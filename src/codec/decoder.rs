use crate::{Error, Result};

#[derive(Debug)]
pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        let bytes = self.read_exact(1)?;
        Ok(bytes[0])
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        let bytes = self.read_exact(2)?;

        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_exact(4)?;

        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        let bytes = self.read_exact(8)?;

        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn read_i64(&mut self) -> Result<i64> {
        let bytes = self.read_exact(8)?;

        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn read_bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.read_u32()? as usize;

        self.read_exact(len)
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos == self.buf.len()
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| Error::InvalidFormat("decoder position overflow".into()))?;

        if end > self.buf.len() {
            return Err(Error::InvalidFormat("unexpected end of input".into()));
        }

        let bytes = &self.buf[self.pos..end];

        self.pos = end;

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Encoder;

    #[test]
    fn read_u8() {
        let bytes = [42];

        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_u8().unwrap(), 42);
        assert_eq!(decoder.remaining(), 0);
    }

    #[test]
    fn read_u16() {
        let mut encoder = Encoder::new();

        encoder.write_u16(42);

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_u16().unwrap(), 42);
    }

    #[test]
    fn read_u32() {
        let mut encoder = Encoder::new();

        encoder.write_u32(0xDEADBEEF);

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_u32().unwrap(), 0xDEADBEEF);
    }

    #[test]
    fn read_u64() {
        let mut encoder = Encoder::new();

        encoder.write_u64(0xDEADBEEFCAFEBABE);

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_u64().unwrap(), 0xDEADBEEFCAFEBABE);
    }

    #[test]
    fn read_i64() {
        let mut encoder = Encoder::new();

        encoder.write_i64(-12345);

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_i64().unwrap(), -12345);
    }

    #[test]
    fn read_bytes() {
        let mut encoder = Encoder::new();

        encoder.write_bytes(b"hello").unwrap();

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_bytes().unwrap(), b"hello");
        assert_eq!(decoder.remaining(), 0);
    }

    #[test]
    fn read_multiple_values() {
        let mut encoder = Encoder::new();

        encoder.write_u32(42);
        encoder.write_u64(100);
        encoder.write_bytes(b"hello").unwrap();

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert_eq!(decoder.read_u32().unwrap(), 42);
        assert_eq!(decoder.read_u64().unwrap(), 100);
        assert_eq!(decoder.read_bytes().unwrap(), b"hello");

        assert!(decoder.is_empty());
    }

    #[test]
    fn truncated_u64_returns_error() {
        let bytes = [1, 2, 3, 4];

        let mut decoder = Decoder::new(&bytes);

        assert!(decoder.read_u64().is_err());
    }

    #[test]
    fn truncated_bytes_returns_error() {
        let mut encoder = Encoder::new();

        encoder.write_u32(100);

        let bytes = encoder.into_inner();
        let mut decoder = Decoder::new(&bytes);

        assert!(decoder.read_bytes().is_err());
    }

    #[test]
    fn empty_input_returns_error() {
        let bytes = [];

        let mut decoder = Decoder::new(&bytes);

        assert!(decoder.read_u8().is_err());
        assert!(decoder.read_u32().is_err());
        assert!(decoder.read_u64().is_err());
    }
}
