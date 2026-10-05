use std::{
    fs::{File, OpenOptions},
    io::Error as IoError,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{Error, Result, block::BlockBuilder, sstable::{Footer, IndexEntry}};

const DEFAULT_BLOCK_SIZE: usize = 4096;

pub struct SstableBuilder {
    file: File,
    path: PathBuf,

    block: BlockBuilder,
    block_size: usize,

    index: Vec<IndexEntry>,
    last_key: Option<Vec<u8>>,

    offset: u64,
}

impl SstableBuilder {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Self::with_block_size(path, DEFAULT_BLOCK_SIZE)
    }

    pub fn with_block_size(path: impl AsRef<Path>, block_size: usize) -> Result<Self> {
        if block_size == 0 {
            return Err(Error::InvalidFormat(
                "SSTable block size must be greater than zero".into(),
            ));
        }

        let path = path.as_ref().to_path_buf();

        let file =
            OpenOptions::new().create(true).append(true).write(true).read(true).open(&path)?;

        Ok(Self {
            file,
            path,
            block: BlockBuilder::with_capacity(block_size),
            block_size,
            index: Vec::new(),
            last_key: None,
            offset: 0,
        })
    }

    pub fn add(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if let Some(last_key) = &self.last_key {
            if key <= last_key.as_slice() {
                return Err(Error::InvalidFormat(
                    "SSTable keys must be strictly increasing".into(),
                ));
            }
        }

        if !self.block.is_empty() && self.block_size + key.len() + value.len() > self.block_size {
            self.flush_block()?;
        }

        self.block.add(key, value)?;
        self.last_key = Some(key.to_vec());

        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        if !self.block.is_empty() {
            self.flush_block()?;
        }

        let index_offset = self.offset;

        let index_data = self.build_index()?;

        self.write_all(&index_data)?;

        let index_size = index_data.len() as u64;

        self.offset += index_size;

        let footer = Footer::new(index_offset, index_size);

        let footer_bytes = footer.encode();

        self.write_all(&footer_bytes)?;

        self.file.sync_all()?;

        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn flush_block(&mut self) -> Result<()> {
        let last_key = self
            .last_key
            .clone()
            .ok_or_else(|| Error::InvalidFormat("cannot flush empty SSTable block".into()))?;

        let block = std::mem::take(&mut self.block);
        let data = block.finish();

        let size = data.len() as u64;

        self.write_all(&data)?;

        self.index.push(IndexEntry { last_key, offset: self.offset, size });

        self.offset += size;

        self.block = BlockBuilder::with_capacity(self.block_size);

        Ok(())
    }

    fn build_index(&self) -> Result<Vec<u8>> {
        let mut builder = BlockBuilder::with_capacity(self.block_size);

        for entry in &self.index {
            let mut value = Vec::with_capacity(16);

            value.extend_from_slice(&entry.offset.to_le_bytes());

            value.extend_from_slice(&entry.size.to_le_bytes());

            builder.add(&entry.last_key, &value)?;
        }

        Ok(builder.finish())
    }

    fn write_all(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            match self.file.write(data) {
                Ok(0) => {
                    return Err(IoError::new(
                        std::io::ErrorKind::WriteZero,
                        "SSTable write returned zero bytes",
                    )
                    .into());
                }
                Ok(n) => data = &data[n..],
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();

        path.push(format!("lsmith-sstable-builder-{}-{}", name, std::process::id()));

        path
    }

    #[test]
    fn create_empty_sstable() {
        let path = temp_path("empty");

        let builder = SstableBuilder::create(&path).unwrap();

        builder.finish().unwrap();

        assert!(path.exists());

        let metadata = fs::metadata(&path).unwrap();

        assert_eq!(metadata.len(), super::super::metadata::FOOTER_SIZE as u64);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn add_entries_and_finish() {
        let path = temp_path("entries");

        let mut builder = SstableBuilder::create(&path).unwrap();

        builder.add(b"apple", b"red").unwrap();
        builder.add(b"banana", b"yellow").unwrap();
        builder.add(b"cherry", b"red").unwrap();

        builder.finish().unwrap();

        assert!(fs::metadata(&path).unwrap().len() > 24);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn keys_must_be_sorted() {
        let path = temp_path("sorted");

        let mut builder = SstableBuilder::create(&path).unwrap();

        builder.add(b"banana", b"2").unwrap();

        let result = builder.add(b"apple", b"1");

        assert!(matches!(result, Err(Error::InvalidFormat(_))));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let path = temp_path("duplicate");

        let mut builder = SstableBuilder::create(&path).unwrap();

        builder.add(b"key", b"one").unwrap();

        let result = builder.add(b"key", b"two");

        assert!(matches!(result, Err(Error::InvalidFormat(_))));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn multiple_blocks_are_created() {
        let path = temp_path("blocks");

        let mut builder = SstableBuilder::with_block_size(&path, 64).unwrap();

        for i in 0..100 {
            builder.add(format!("key-{i:03}").as_bytes(), b"value").unwrap();
        }

        builder.finish().unwrap();

        let size = fs::metadata(&path).unwrap().len();

        assert!(size > 64);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn large_entry_is_allowed() {
        let path = temp_path("large");

        let mut builder = SstableBuilder::with_block_size(&path, 64).unwrap();

        let value = vec![0xAB; 1024];

        builder.add(b"large", &value).unwrap();
        builder.finish().unwrap();

        assert!(fs::metadata(&path).unwrap().len() > 1024);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn empty_values_are_allowed() {
        let path = temp_path("empty-value");

        let mut builder = SstableBuilder::create(&path).unwrap();

        builder.add(b"key", b"").unwrap();
        builder.add(b"next", b"value").unwrap();

        builder.finish().unwrap();

        assert!(fs::metadata(&path).unwrap().len() > 24);

        fs::remove_file(path).unwrap();
    }
}
