use crate::{Result, codec::Decoder, sstable::SstableReader};

pub struct SstableIter<'a> {
    reader: &'a mut SstableReader,
    index_pos: usize,
    block: Vec<u8>,
    block_pos: usize
}

impl<'a> SstableIter<'a> {
    pub(crate) fn new(reader: &'a mut SstableReader) -> Self {
        Self { reader, index_pos: 0, block: Vec::new(), block_pos: 0 }
    }

    fn load_next_block(&mut self) -> Result<bool> {
        if self.index_pos >= self.reader.index_len() {
            return Ok(false);
        }

        let (offset, size) = self.reader.index_entry(self.index_pos);
        self.index_pos += 1;

        self.block = self.reader.read_block(offset, size)?;
        self.block_pos = 0;

        Ok(true)
    }

    fn next_entry(&mut self) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        loop {
            if self.block_pos < self.block.len() {
                let remaining = &self.block[self.block_pos..];
                let mut decoder = Decoder::new(remaining);

                let key = decoder.read_bytes()?.to_vec();
                let value = decoder.read_bytes()?.to_vec();

                self.block_pos += decoder.position();

                return Ok(Some((key, value)));
            }

            if !self.load_next_block()? {
                return Ok(None);    
            }
        }

    }
}

impl<'a> Iterator for SstableIter<'a> {
    type Item = Result<(Vec<u8>, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_entry() {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => None,
            Err(error) => {
                self.block_pos = self.block.len();
                Some(Err(error))
            }
        }
    }
}
