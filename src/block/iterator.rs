use crate::{Result, codec::Decoder};

pub struct BlockIter<'a> {
    pub(crate) decoder: Decoder<'a>,
}

impl<'a> Iterator for BlockIter<'a> {
    type Item = Result<(&'a [u8], &'a [u8])>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.decoder.is_empty() {
            return None;
        }

        let key = match self.decoder.read_bytes() {
            Ok(key) => key,
            Err(error) => return Some(Err(error)),
        };

        let value = match self.decoder.read_bytes() {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };

        Some(Ok((key, value)))
    }
}
