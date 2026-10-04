use crate::{
    Error, Result,
    codec::{Decoder, Encoder},
    types::RecordKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    pub sequence: u64,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub kind: RecordKind,
}

impl WalRecord {
    pub fn put(sequence: u64, key: Vec<u8>, value: Vec<u8>) -> Self {
        Self { sequence, key, value, kind: RecordKind::Put }
    }

    pub fn delete(sequence: u64, key: Vec<u8>) -> Self {
        Self { sequence, key, value: Vec::new(), kind: RecordKind::Delete }
    }

    pub fn encode_payload(&self) -> Result<Vec<u8>> {
        let mut encoder = Encoder::new();

        encoder.write_u64(self.sequence);
        encoder.write_u8(match self.kind {
            RecordKind::Put => 0,
            RecordKind::Delete => 1,
        });

        encoder.write_bytes(&self.key)?;

        match self.kind {
            RecordKind::Put => encoder.write_bytes(&self.value)?,
            RecordKind::Delete => encoder.write_u32(0),
        }

        Ok(encoder.into_inner())
    }

    pub fn decode_payload(payload: &[u8]) -> Result<Self> {
        let mut decoder = Decoder::new(payload);

        let sequence = decoder.read_u64()?;

        let kind = match decoder.read_u8()? {
            0 => RecordKind::Put,
            1 => RecordKind::Delete,
            _ => return Err(Error::InvalidFormat("invalid WAL record kind".into())),
        };

        let key = decoder.read_bytes()?.to_vec();
        let value = decoder.read_bytes()?.to_vec();

        if !decoder.is_empty() {
            return Err(Error::InvalidFormat("trailing bytes in WAL record".into()));
        }

        if kind == RecordKind::Delete && !value.is_empty() {
            return Err(Error::InvalidFormat("delete WAL record contains a value".into()));
        }

        Ok(Self { sequence, key, value, kind })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_round_trip() {
        let record = WalRecord::put(42, b"hello".to_vec(), b"world".to_vec());

        let encoded = record.encode_payload().unwrap();
        let decoded = WalRecord::decode_payload(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn delete_round_trip() {
        let record = WalRecord::delete(100, b"hello".to_vec());

        let encoded = record.encode_payload().unwrap();
        let decoded = WalRecord::decode_payload(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn empty_key_and_value() {
        let record = WalRecord::put(1, Vec::new(), Vec::new());

        let encoded = record.encode_payload().unwrap();
        let decoded = WalRecord::decode_payload(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn large_value() {
        let record = WalRecord::put(123, b"key".to_vec(), vec![0xAB; 1024 * 1024]);

        let encoded = record.encode_payload().unwrap();
        let decoded = WalRecord::decode_payload(&encoded).unwrap();

        assert_eq!(decoded, record);
    }

    #[test]
    fn invalid_kind_is_rejected() {
        let mut encoder = Encoder::new();

        encoder.write_u64(1);
        encoder.write_u8(99);
        encoder.write_bytes(b"key").unwrap();
        encoder.write_bytes(b"value").unwrap();

        let result = WalRecord::decode_payload(encoder.as_slice());

        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn truncated_payload_is_rejected() {
        let record = WalRecord::put(1, b"key".to_vec(), b"value".to_vec());

        let mut encoded = record.encode_payload().unwrap();
        encoded.pop();

        assert!(WalRecord::decode_payload(&encoded).is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let record = WalRecord::put(1, b"key".to_vec(), b"value".to_vec());

        let mut encoded = record.encode_payload().unwrap();
        encoded.extend_from_slice(&[1, 2, 3]);

        assert!(matches!(WalRecord::decode_payload(&encoded), Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn delete_cannot_contain_value() {
        let mut encoder = Encoder::new();

        encoder.write_u64(1);
        encoder.write_u8(1);
        encoder.write_bytes(b"key").unwrap();
        encoder.write_bytes(b"value").unwrap();

        assert!(matches!(
            WalRecord::decode_payload(encoder.as_slice()),
            Err(Error::InvalidFormat(_))
        ));
    }
}
