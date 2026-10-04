use std::{fs::File, io::Read, path::Path};

use crate::{
    Error, Result, crc32,
    wal::{HEADER_SIZE, WalRecord},
};

pub struct WalReader;

impl WalReader {
    pub fn read_all(path: impl AsRef<Path>) -> Result<Vec<WalRecord>> {
        let mut file = File::open(path)?;
        let mut records = Vec::new();

        loop {
            let mut header = [0u8; HEADER_SIZE];

            match read_header(&mut file, &mut header)? {
                HeaderResult::Eof | HeaderResult::Truncated => break,
                HeaderResult::Complete => {}
            }

            let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;

            let expected_checksum =
                u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

            let mut payload = vec![0u8; length];

            match read_payload(&mut file, &mut payload)? {
                PayloadResult::Complete => {}
                PayloadResult::Truncated => break,
            }

            let actual_checksum = crc32::checksum(&payload);

            if actual_checksum != expected_checksum {
                return Err(Error::ChecksumMismatch);
            }

            let record = WalRecord::decode_payload(&payload)?;

            records.push(record);
        }

        Ok(records)
    }
}

enum HeaderResult {
    Eof,
    Complete,
    Truncated,
}

fn read_header(file: &mut File, header: &mut [u8; HEADER_SIZE]) -> Result<HeaderResult> {
    let mut read = 0;

    while read < HEADER_SIZE {
        match file.read(&mut header[read..]) {
            Ok(0) if read == 0 => return Ok(HeaderResult::Eof),
            Ok(0) => return Ok(HeaderResult::Truncated),
            Ok(n) => read += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Ok(HeaderResult::Complete)
}

enum PayloadResult {
    Complete,
    Truncated,
}

fn read_payload(file: &mut File, payload: &mut [u8]) -> Result<PayloadResult> {
    let mut read = 0;

    while read < payload.len() {
        match file.read(&mut payload[read..]) {
            Ok(0) => return Ok(PayloadResult::Truncated),
            Ok(n) => read += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Ok(PayloadResult::Complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::WalWriter;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();

        path.push(format!("lsmith-wal-reader-test-{}-{}", name, std::process::id()));

        path
    }

    #[test]
    fn read_empty_wal() {
        let path = temp_path("empty");

        File::create(&path).unwrap();

        let records = WalReader::read_all(&path).unwrap();

        assert!(records.is_empty());

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_single_record() {
        let path = temp_path("single");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(1, b"hello".to_vec(), b"world".to_vec())).unwrap();
        }

        let records = WalReader::read_all(&path).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0], WalRecord::put(1, b"hello".to_vec(), b"world".to_vec(),));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_multiple_records() {
        let path = temp_path("multiple");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            for i in 0..100 {
                wal.append(&WalRecord::put(
                    i,
                    format!("key-{i}").into_bytes(),
                    format!("value-{i}").into_bytes(),
                ))
                .unwrap();
            }
        }

        let records = WalReader::read_all(&path).unwrap();

        assert_eq!(records.len(), 100);

        for i in 0..100 {
            assert_eq!(records[i].sequence, i as u64);
        }

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_put_and_delete() {
        let path = temp_path("delete");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(1, b"key".to_vec(), b"value".to_vec())).unwrap();

            wal.append(&WalRecord::delete(2, b"key".to_vec())).unwrap();
        }

        let records = WalReader::read_all(&path).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].kind, crate::types::RecordKind::Put);
        assert_eq!(records[1].kind, crate::types::RecordKind::Delete);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn truncated_header_is_ignored_as_tail() {
        let path = temp_path("truncated-header");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(1, b"a".to_vec(), b"one".to_vec())).unwrap();

            wal.append(&WalRecord::put(2, b"b".to_vec(), b"two".to_vec())).unwrap();
        }

        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();

        let size = file.metadata().unwrap().len();

        file.set_len(size - 3).unwrap();

        drop(file);

        let records = WalReader::read_all(&path).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sequence, 1);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn truncated_payload_is_ignored_as_tail() {
        let path = temp_path("truncated-payload");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(1, b"a".to_vec(), b"one".to_vec())).unwrap();

            wal.append(&WalRecord::put(2, b"b".to_vec(), b"two".to_vec())).unwrap();
        }

        let mut file = fs::OpenOptions::new().write(true).open(&path).unwrap();

        let size = file.metadata().unwrap().len();

        file.set_len(size - 2).unwrap();

        drop(file);

        let records = WalReader::read_all(&path).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sequence, 1);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn checksum_corruption_is_detected() {
        let path = temp_path("checksum");

        {
            let mut wal = WalWriter::create(&path).unwrap();

            wal.append(&WalRecord::put(1, b"key".to_vec(), b"value".to_vec())).unwrap();
        }

        let mut bytes = fs::read(&path).unwrap();

        // Corrupt the payload, not the header.
        bytes[10] ^= 0xFF;

        fs::write(&path, bytes).unwrap();

        let result = WalReader::read_all(&path);

        assert!(matches!(result, Err(Error::ChecksumMismatch)));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn invalid_payload_is_rejected() {
        let path = temp_path("invalid-payload");

        let mut file = File::create(&path).unwrap();

        let mut payload = Vec::new();

        payload.extend_from_slice(&1u64.to_le_bytes());
        payload.push(99);

        let checksum = crc32::checksum(&payload);

        file.write_all(&(payload.len() as u32).to_le_bytes()).unwrap();

        file.write_all(&checksum.to_le_bytes()).unwrap();

        file.write_all(&payload).unwrap();

        drop(file);

        let result = WalReader::read_all(&path);

        assert!(matches!(result, Err(Error::InvalidFormat(_))));

        fs::remove_file(path).unwrap();
    }
}
