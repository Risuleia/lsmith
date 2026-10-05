mod builder;
mod iterator;
mod metadata;
mod reader;

pub use builder::SstableBuilder;
pub use metadata::{Footer, FOOTER_SIZE};
pub use reader::SstableReader;
pub use iterator::SstableIter;

#[derive(Debug)]
struct IndexEntry {
    last_key: Vec<u8>,
    offset: u64,
    size: u64,
}
