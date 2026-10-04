mod reader;
mod record;
mod writer;

const HEADER_SIZE: usize = 8;

pub use record::WalRecord;
pub use writer::WalWriter;
pub use reader::WalReader;