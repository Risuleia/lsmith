use crate::{
    Error, Result,
    codec::{Decoder, Encoder},
};

pub const FOOTER_SIZE: usize = 24;

const MAGIC: u64 = 0x4C53_4D49_5448_3031;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footer {
    pub index_offset: u64,
    pub index_size: u64,
}

impl Footer {
    pub fn new(index_offset: u64, index_size: u64) -> Self {
        Self { index_offset, index_size }
    }

    pub fn encode(self) -> [u8; FOOTER_SIZE] {
        let mut encoder = Encoder::with_capacity(FOOTER_SIZE);

        encoder.write_u64(self.index_offset);
        encoder.write_u64(self.index_size);
        encoder.write_u64(MAGIC);

        let bytes = encoder.into_inner();

        let mut result = [0u8; FOOTER_SIZE];
        result.copy_from_slice(&bytes);

        result
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != FOOTER_SIZE {
            return Err(Error::InvalidFormat("invalid SSTable footer size".into()));
        }

        let mut decoder = Decoder::new(bytes);

        let index_offset = decoder.read_u64()?;
        let index_size = decoder.read_u64()?;
        let magic = decoder.read_u64()?;

        if magic != MAGIC {
            return Err(Error::InvalidFormat("invalid SSTable magic number".into()));
        }

        if !decoder.is_empty() {
            return Err(Error::InvalidFormat("trailing bytes in SSTable footer".into()));
        }

        Ok(Self { index_offset, index_size })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_round_trip() {
        let footer = Footer::new(1234, 5678);

        let encoded = footer.encode();

        assert_eq!(encoded.len(), FOOTER_SIZE);

        let decoded = Footer::decode(&encoded).unwrap();

        assert_eq!(decoded, footer);
    }

    #[test]
    fn footer_has_fixed_size() {
        let footer = Footer::new(0, 0);

        assert_eq!(footer.encode().len(), FOOTER_SIZE);
    }

    #[test]
    fn invalid_footer_size_is_rejected() {
        let result = Footer::decode(&[0u8; 10]);

        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn invalid_magic_is_rejected() {
        let footer = Footer::new(100, 200);
        let mut bytes = footer.encode();

        bytes[16] ^= 0xFF;

        let result = Footer::decode(&bytes);

        assert!(matches!(result, Err(Error::InvalidFormat(_))));
    }

    #[test]
    fn footer_supports_large_offsets() {
        let footer = Footer::new(u64::MAX - 100, u64::MAX - 200);

        let decoded = Footer::decode(&footer.encode()).unwrap();

        assert_eq!(decoded, footer);
    }
}
