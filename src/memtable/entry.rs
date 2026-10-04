use crate::types::{InternalKey, RecordKind};

pub struct MemtableEntry {
    pub key: InternalKey,
    pub value: Vec<u8>,
}

impl MemtableEntry {
    pub fn put(key: Vec<u8>, value: Vec<u8>, sequence: u64) -> Self {
        Self { key: InternalKey::put(key, sequence), value }
    }

    pub fn delete(key: Vec<u8>, sequence: u64) -> Self {
        Self { key: InternalKey::delete(key, sequence), value: Vec::new() }
    }

    pub fn is_deleted(&self) -> bool {
        self.key.kind == RecordKind::Delete
    }

    pub fn memory_usage(&self) -> usize {
        self.key.user_key.len()
            + self.value.len()
            + std::mem::size_of::<InternalKey>()
            + std::mem::size_of::<Vec<u8>>()
    }
}
