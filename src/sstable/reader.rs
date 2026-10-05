use std::{
    fs::File,
    io::{Read, Seek},
    path::Path,
};

use crate::{
    Error, Result,
    block::BlockReader,
    sstable::{FOOTER_SIZE, Footer, IndexEntry},
};

#[derive(Debug)]
pub struct SstableReader {
    file: File,
    footer: Footer,
    index: Vec<IndexEntry>,
}

impl SstableReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path)?;

        let file_size = file.metadata()?.len();

        if file_size < FOOTER_SIZE as u64 {
            return Err(Error::InvalidFormat("SSTable is smaller than its footer".into()));
        }

        file.seek(std::io::SeekFrom::End(-(FOOTER_SIZE as i64)))?;

        let mut footer_bytes = [0u8; FOOTER_SIZE];

        read_exact(&mut file, &mut footer_bytes)?;

        let footer = Footer::decode(&footer_bytes)?;

        let index_end = footer
            .index_offset
            .checked_add(footer.index_size)
            .ok_or_else(|| Error::Corruption("SSTable index offset overflow".into()))?;

        if index_end > file_size - FOOTER_SIZE as u64 {
            return Err(Error::Corruption("SSTable index lies outside file".into()));
        }

        file.seek(std::io::SeekFrom::Start(footer.index_offset))?;

        let index_size = usize::try_from(footer.index_size)
            .map_err(|_| Error::InvalidFormat("SSTable index is too large".into()))?;

        let mut index_data = vec![0u8; index_size];

        read_exact(&mut file, &mut index_data);

        let index_block = BlockReader::new(&index_data)?;

        let mut index = Vec::new();

        for entry in index_block.iter() {
            let (last_key, metadata) = entry?;

            if metadata.len() != 16 {
                return Err(Error::InvalidFormat("invalid SSTable index entry".into()));
            }

            let mut offset_bytes = [0u8; 8];
            let mut size_bytes = [0u8; 8];

            offset_bytes.copy_from_slice(&metadata[..8]);
            size_bytes.copy_from_slice(&metadata[8..]);

            let offset = u64::from_le_bytes(offset_bytes);
            let size = u64::from_le_bytes(size_bytes);

            index.push(IndexEntry { last_key: last_key.to_vec(), offset, size });
        }

        Ok(Self { file, footer, index })
    }

    pub fn get(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let (offset, size) = match self.find_block(key) {
            Some(entry) => (entry.offset, entry.size),
            None => return Ok(None),
        };

        let data = self.read_block(offset, size)?;

        let block = BlockReader::new(&data)?;

        match block.get(key)? {
            Some(value) => Ok(Some(value.to_vec())),
            None => Ok(None),
        }
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn footer(&self) -> Footer {
        self.footer
    }

    pub fn iter(&mut self) -> super::iterator::SstableIter<'_> {
        super::iterator::SstableIter::new(self)
    }

    pub(crate) fn index_len(&self) -> usize {
        self.index.len()
    }

    pub(crate) fn index_entry(&self, index: usize) -> (u64, u64) {
        let entry = &self.index[index];
        (entry.offset, entry.size)
    }

    fn find_block(&self, key: &[u8]) -> Option<&IndexEntry> {
        self.index.iter().find(|entry| key <= entry.last_key.as_slice())
    }

    pub fn read_block(&mut self, offset: u64, size: u64) -> Result<Vec<u8>> {
        let file_size = self.file.metadata()?.len();

        let end = offset
            .checked_add(size)
            .ok_or_else(|| Error::Corruption("SSTable block offset overflow".into()))?;

        if end > file_size {
            return Err(Error::Corruption("SSTable block lies outside file".into()));
        }

        self.file.seek(std::io::SeekFrom::Start(offset))?;

        let size = usize::try_from(size)
            .map_err(|_| Error::InvalidFormat("SSTable block is too large".into()))?;

        let mut data = vec![0u8; size];

        read_exact(&mut self.file, &mut data)?;

        Ok(data)
    }
}

fn read_exact(file: &mut File, buffer: &mut [u8]) -> Result<()> {
    let mut read = 0;

    while read < buffer.len() {
        match file.read(&mut buffer[read..]) {
            Ok(0) => return Err(Error::InvalidFormat("unexpected end of SSTable".into())),
            Ok(n) => read += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sstable::SstableBuilder;
    use std::fs;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();

        path.push(format!("lsmith-sstable-reader-{}-{}", name, std::process::id()));

        path
    }

    fn create_sstable(path: &Path) {
        let mut builder = SstableBuilder::with_block_size(path, 64).unwrap();

        builder.add(b"apple", b"red").unwrap();
        builder.add(b"banana", b"yellow").unwrap();
        builder.add(b"cherry", b"red").unwrap();
        builder.add(b"date", b"brown").unwrap();
        builder.add(b"elderberry", b"purple").unwrap();

        builder.finish().unwrap();
    }

    #[test]
    fn open_sstable() {
        let path = temp_path("open");

        create_sstable(&path);

        let reader = SstableReader::open(&path).unwrap();

        assert!(!reader.is_empty());
        assert!(reader.len() > 0);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn get_existing_keys() {
        let path = temp_path("get");

        create_sstable(&path);

        let mut reader = SstableReader::open(&path).unwrap();

        assert_eq!(reader.get(b"apple").unwrap(), Some(b"red".to_vec()));

        assert_eq!(reader.get(b"banana").unwrap(), Some(b"yellow".to_vec()));

        assert_eq!(reader.get(b"cherry").unwrap(), Some(b"red".to_vec()));

        assert_eq!(reader.get(b"date").unwrap(), Some(b"brown".to_vec()));

        assert_eq!(reader.get(b"elderberry").unwrap(), Some(b"purple".to_vec()));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_key_returns_none() {
        let path = temp_path("missing");

        create_sstable(&path);

        let mut reader = SstableReader::open(&path).unwrap();

        assert_eq!(reader.get(b"grape").unwrap(), None);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn key_before_first_returns_none() {
        let path = temp_path("before");

        create_sstable(&path);

        let mut reader = SstableReader::open(&path).unwrap();

        assert_eq!(reader.get(b"aardvark").unwrap(), None);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn key_after_last_returns_none() {
        let path = temp_path("after");

        create_sstable(&path);

        let mut reader = SstableReader::open(&path).unwrap();

        assert_eq!(reader.get(b"zebra").unwrap(), None);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn lookup_across_multiple_blocks() {
        let path = temp_path("multi-block");

        create_sstable(&path);

        let mut reader = SstableReader::open(&path).unwrap();

        assert_eq!(reader.get(b"apple").unwrap(), Some(b"red".to_vec()));

        assert_eq!(reader.get(b"date").unwrap(), Some(b"brown".to_vec()));

        assert_eq!(reader.get(b"elderberry").unwrap(), Some(b"purple".to_vec()));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn truncated_sstable_is_rejected() {
        let path = temp_path("truncated");

        create_sstable(&path);

        let size = fs::metadata(&path).unwrap().len();

        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();

        file.set_len(size - 5).unwrap();

        drop(file);

        assert!(SstableReader::open(&path).is_err());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_magic_is_rejected() {
        let path = temp_path("magic");

        create_sstable(&path);

        let mut bytes = fs::read(&path).unwrap();

        let len = bytes.len();

        bytes[len - 1] ^= 0xFF;

        fs::write(&path, bytes).unwrap();

        assert!(SstableReader::open(&path).is_err());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_sstable_can_be_opened() {
        let path = temp_path("empty");

        let builder = SstableBuilder::create(&path).unwrap();

        builder.finish().unwrap();

        let reader = SstableReader::open(&path).unwrap();

        assert!(reader.is_empty());

        let mut reader = reader;

        assert_eq!(reader.get(b"anything").unwrap(), None);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn iterates_all_entries() {
        let dir = std::env::temp_dir();
        let path = dir.join("lsmith-sstable-iterator-test");

        let mut builder = SstableBuilder::with_block_size(&path, 20).unwrap();

        builder.add(b"apple", b"red").unwrap();
        builder.add(b"banana", b"yellow").unwrap();
        builder.add(b"carrot", b"orange").unwrap();
        builder.add(b"date", b"brown").unwrap();
        builder.add(b"eggplant", b"purple").unwrap();

        builder.finish().unwrap();

        let mut reader = SstableReader::open(&path).unwrap();

        let entries: Vec<_> = reader.iter().collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(
            entries,
            vec![
                (b"apple".to_vec(), b"red".to_vec()),
                (b"banana".to_vec(), b"yellow".to_vec()),
                (b"carrot".to_vec(), b"orange".to_vec()),
                (b"date".to_vec(), b"brown".to_vec()),
                (b"eggplant".to_vec(), b"purple".to_vec()),
            ]
        );

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn iterates_empty_table() {
        let dir = std::env::temp_dir();
        let path = dir.join("lsmith-sstable-empty-iterator-test");

        let builder = SstableBuilder::create(&path).unwrap();
        builder.finish().unwrap();

        let mut reader = SstableReader::open(&path).unwrap();

        assert_eq!(reader.iter().next().transpose().unwrap(), None);

        std::fs::remove_file(path).unwrap();
    }
}
