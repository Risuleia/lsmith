use std::cmp::Ordering;

use crate::{Error, Result, block::iterator::BlockIter, codec::Decoder};

#[derive(Debug)]
pub struct BlockReader<'a> {
    data: &'a [u8],
    entries: usize,
}

impl<'a> BlockReader<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let mut decoder = Decoder::new(data);
        let mut entries = 0;
        let mut prev_key: Option<&[u8]> = None;

        while !decoder.is_empty() {
            let key = decoder.read_bytes()?;
            decoder.read_bytes()?;

            if let Some(prev) = prev_key {
                if key <= prev {
                    return Err(Error::InvalidFormat(
                        "block keys are not strictly increasing".into(),
                    ));
                }
            }

            prev_key = Some(key);
            entries += 1;
        }

        Ok(Self { data, entries })
    }

    pub fn len(&self) -> usize {
        self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries == 0
    }

    pub fn get(&self, target: &[u8]) -> Result<Option<&'a [u8]>> {
        let mut decoder = Decoder::new(self.data);

        while !decoder.is_empty() {
            let key = decoder.read_bytes()?;
            let value = decoder.read_bytes()?;

            match key.cmp(target) {
                Ordering::Less => {}
                Ordering::Equal => return Ok(Some(value)),
                Ordering::Greater => return Ok(None),
            }
        }

        Ok(None)
    }

    pub fn iter(&self) -> BlockIter<'a> {
        BlockIter { decoder: Decoder::new(self.data) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockBuilder;

    fn make_block() -> Vec<u8> {
        let mut builder = BlockBuilder::new();

        builder.add(b"apple", b"red").unwrap();
        builder.add(b"banana", b"yellow").unwrap();
        builder.add(b"cherry", b"red").unwrap();

        builder.finish()
    }

    #[test]
    fn read_empty_block() {
        let data = BlockBuilder::new().finish();

        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.len(), 0);
        assert!(reader.is_empty());
    }

    #[test]
    fn read_entries() {
        let data = make_block();

        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.len(), 3);
    }

    #[test]
    fn get_existing_key() {
        let data = make_block();
        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.get(b"apple").unwrap(), Some(b"red".as_slice()));

        assert_eq!(reader.get(b"banana").unwrap(), Some(b"yellow".as_slice()));

        assert_eq!(reader.get(b"cherry").unwrap(), Some(b"red".as_slice()));
    }

    #[test]
    fn get_missing_key() {
        let data = make_block();
        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.get(b"grape").unwrap(), None);
    }

    #[test]
    fn get_before_first_key() {
        let data = make_block();
        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.get(b"aardvark").unwrap(), None);
    }

    #[test]
    fn get_between_keys() {
        let data = make_block();
        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.get(b"blueberry").unwrap(), None);
    }

    #[test]
    fn iterator_returns_entries_in_order() {
        let data = make_block();
        let reader = BlockReader::new(&data).unwrap();

        let entries: Vec<_> = reader.iter().map(|entry| entry.unwrap()).collect();

        assert_eq!(
            entries,
            vec![
                (b"apple".as_slice(), b"red".as_slice()),
                (b"banana".as_slice(), b"yellow".as_slice()),
                (b"cherry".as_slice(), b"red".as_slice()),
            ]
        );
    }

    #[test]
    fn truncated_key_is_rejected() {
        let data = vec![10, 0, 0, 0, b'a', b'b'];

        assert!(BlockReader::new(&data).is_err());
    }

    #[test]
    fn truncated_value_is_rejected() {
        let mut builder = BlockBuilder::new();

        builder.add(b"key", b"value").unwrap();

        let mut data = builder.finish();
        data.pop();

        assert!(BlockReader::new(&data).is_err());
    }

    #[test]
    fn malformed_length_is_rejected() {
        let data = vec![0xFF, 0xFF, 0xFF, 0x7F];

        assert!(BlockReader::new(&data).is_err());
    }

    #[test]
    fn empty_values_are_read_correctly() {
        let mut builder = BlockBuilder::new();

        builder.add(b"key", b"").unwrap();

        let data = builder.finish();
        let reader = BlockReader::new(&data).unwrap();

        assert_eq!(reader.get(b"key").unwrap(), Some(b"".as_slice()));
    }

    #[test]
    fn unsorted_block_is_rejected() {
        let mut encoder = crate::codec::Encoder::new();

        encoder.write_bytes(b"banana").unwrap();
        encoder.write_bytes(b"yellow").unwrap();

        encoder.write_bytes(b"apple").unwrap();
        encoder.write_bytes(b"red").unwrap();

        let data = encoder.into_inner();

        assert!(matches!(BlockReader::new(&data), Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let mut encoder = crate::codec::Encoder::new();

        encoder.write_bytes(b"apple").unwrap();
        encoder.write_bytes(b"one").unwrap();

        encoder.write_bytes(b"apple").unwrap();
        encoder.write_bytes(b"two").unwrap();

        let data = encoder.into_inner();

        assert!(matches!(BlockReader::new(&data), Err(Error::InvalidFormat(_))));
    }
}
