use crate::{Error, Result, codec::Encoder};

#[derive(Debug)]
pub struct BlockBuilder {
    buf: Encoder,
    entries: usize,
    last_key: Option<Vec<u8>>,
    finished: bool,
}

impl BlockBuilder {
    pub fn new() -> Self {
        Self::with_capacity(4096)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { buf: Encoder::with_capacity(capacity), entries: 0, last_key: None, finished: false }
    }

    pub fn add(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        if self.finished {
            return Err(Error::InvalidFormat("cannot add to a finished block".into()));
        }

        if let Some(last_key) = &self.last_key {
            if key <= last_key.as_slice() {
                return Err(Error::InvalidFormat("block keys must be strictly increasing".into()));
            }
        }

        self.buf.write_bytes(key)?;
        self.buf.write_bytes(value)?;

        self.last_key = Some(key.to_vec());
        self.entries += 1;

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries == 0
    }

    pub fn size(&self) -> usize {
        self.buf.len()
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.finished = true;
        self.buf.into_inner()
    }
}

impl Default for BlockBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_block_is_empty() {
        let block = BlockBuilder::new();

        assert!(block.is_empty());
        assert_eq!(block.len(), 0);
        assert_eq!(block.size(), 0);
    }

    #[test]
    fn add_entry() {
        let mut block = BlockBuilder::new();

        block.add(b"apple", b"red").unwrap();

        assert_eq!(block.len(), 1);
        assert!(!block.is_empty());
        assert!(block.size() > 0);
    }

    #[test]
    fn add_multiple_entries() {
        let mut block = BlockBuilder::new();

        block.add(b"apple", b"red").unwrap();
        block.add(b"banana", b"yellow").unwrap();
        block.add(b"cherry", b"red").unwrap();

        assert_eq!(block.len(), 3);
    }

    #[test]
    fn keys_must_be_strictly_increasing() {
        let mut block = BlockBuilder::new();

        block.add(b"apple", b"1").unwrap();
        block.add(b"banana", b"2").unwrap();

        assert!(block.add(b"cherry", b"3").is_ok());
    }

    #[test]
    fn duplicate_keys_are_rejected() {
        let mut block = BlockBuilder::new();

        block.add(b"apple", b"first").unwrap();

        let result = block.add(b"apple", b"second");

        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn decreasing_keys_are_rejected() {
        let mut block = BlockBuilder::new();

        block.add(b"banana", b"1").unwrap();

        let result = block.add(b"apple", b"2");

        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn empty_keys_are_allowed() {
        let mut block = BlockBuilder::new();

        block.add(b"", b"value").unwrap();
        block.add(b"a", b"value").unwrap();

        assert_eq!(block.len(), 2);
    }

    #[test]
    fn empty_values_are_allowed() {
        let mut block = BlockBuilder::new();

        block.add(b"key", b"").unwrap();
        block.add(b"next", b"value").unwrap();

        assert_eq!(block.len(), 2);
    }

    #[test]
    fn large_value() {
        let mut block = BlockBuilder::new();

        let value = vec![0xAB; 1024 * 1024];

        block.add(b"large", &value).unwrap();

        assert_eq!(block.len(), 1);
        assert!(block.size() >= value.len());
    }

    #[test]
    fn finish_returns_encoded_data() {
        let mut block = BlockBuilder::new();

        block.add(b"apple", b"red").unwrap();
        block.add(b"banana", b"yellow").unwrap();

        let data = block.finish();

        assert!(!data.is_empty());
    }

    #[test]
    fn empty_block_can_be_finished() {
        let block = BlockBuilder::new();

        let data = block.finish();

        assert!(data.is_empty());
    }

    #[test]
    fn binary_keys_and_values() {
        let mut block = BlockBuilder::new();

        block.add(&[0x00, 0x01, 0xFF], &[0xAA, 0xBB, 0x00]).unwrap();

        assert_eq!(block.len(), 1);
    }
}
