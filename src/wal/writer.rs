use std::{
    fs::{File, OpenOptions},
    io::Error as IoError,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{Error, Result, crc32, wal::{HEADER_SIZE, WalRecord}};

pub struct WalWriter {
    file: File,
    path: PathBuf,
}

impl WalWriter {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let file = OpenOptions::new().create(true).append(true).read(true).open(&path)?;

        Ok(Self { file, path })
    }

    pub fn append(&mut self, record: &WalRecord) -> Result<()> {
        let payload = record.encode_payload()?;

        let length = u32::try_from(payload.len())
            .map_err(|_| Error::InvalidFormat("WAL record is too large".into()))?;

        let checksum = crc32::checksum(&payload);

        let mut header = [0u8; HEADER_SIZE];

        header[..4].copy_from_slice(&length.to_le_bytes());
        header[4..].copy_from_slice(&checksum.to_le_bytes());

        self.write_all(&header)?;
        self.write_all(&payload)?;

        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        self.file.sync_data()?;
        Ok(())
    }

    pub fn position(&self) -> Result<u64> {
        Ok(self.file.metadata()?.len())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn write_all(&mut self, mut data: &[u8]) -> Result<()> {
        while !data.is_empty() {
            match self.file.write(data) {
                Ok(0) => {
                    return Err(IoError::new(
                        std::io::ErrorKind::WriteZero,
                        "WAL write returned zero bytes",
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
        path.push(format!("lsmith-wal-test-{}-{}", name, std::process::id()));
        path
    }

    #[test]
    fn create_creates_file() {
        let path = temp_path("create");

        let wal = WalWriter::create(&path).unwrap();

        assert!(path.exists());
        assert_eq!(wal.position().unwrap(), 0);

        drop(wal);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn append_increases_position() {
        let path = temp_path("position");

        let mut wal = WalWriter::create(&path).unwrap();

        let record = WalRecord::put(1, b"hello".to_vec(), b"world".to_vec());

        wal.append(&record).unwrap();

        let first_position = wal.position().unwrap();

        wal.append(&record).unwrap();

        let second_position = wal.position().unwrap();

        assert!(second_position > first_position);

        drop(wal);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn multiple_records_are_appended() {
        let path = temp_path("multiple");

        let mut wal = WalWriter::create(&path).unwrap();

        for i in 0..100 {
            let record = WalRecord::put(
                i,
                format!("key-{i}").into_bytes(),
                format!("value-{i}").into_bytes(),
            );

            wal.append(&record).unwrap();
        }

        assert!(wal.position().unwrap() > 0);

        drop(wal);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn reopen_appends_to_existing_file() {
        let path = temp_path("reopen");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(1, b"a".to_vec(), b"one".to_vec())).unwrap();
        }

        let first_size = fs::metadata(&path).unwrap().len();

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(2, b"b".to_vec(), b"two".to_vec())).unwrap();
        }

        let second_size = fs::metadata(&path).unwrap().len();

        assert!(second_size > first_size);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn sync_succeeds() {
        let path = temp_path("sync");

        let mut wal = WalWriter::create(&path).unwrap();

        wal.append(&WalRecord::put(1, b"key".to_vec(), b"value".to_vec())).unwrap();

        wal.sync().unwrap();

        drop(wal);
        fs::remove_file(path).unwrap();
    }
}
