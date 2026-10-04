use crate::{
    collections::SkipList,
    types::{InternalKey, RecordKind},
    wal::WalRecord,
};

pub struct MemTable {
    table: SkipList<InternalKey, Vec<u8>>,
    approximate_size: usize,
}

impl MemTable {
    pub fn new() -> Self {
        Self { table: SkipList::new(), approximate_size: 0 }
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn approximate_size(&self) -> usize {
        self.approximate_size
    }

    pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>, sequence: u64) {
        let internal_key = InternalKey::put(key, sequence);

        self.approximate_size += internal_key.user_key.len() + value.len();

        self.table.insert(internal_key, value);
    }

    pub fn delete(&mut self, key: Vec<u8>, sequence: u64) {
        let internal_key = InternalKey::delete(key, sequence);

        self.approximate_size += internal_key.user_key.len();

        self.table.insert(internal_key, Vec::new());
    }

    pub fn get(&self, key: &[u8], sequence: u64) -> Option<&[u8]> {
        let lookup = InternalKey::put(key.to_vec(), sequence);

        let (internal_key, value) = self.table.lower_bound(&lookup)?;

        if internal_key.user_key != key {
            return None;
        }

        match internal_key.kind {
            RecordKind::Put => Some(value.as_slice()),
            RecordKind::Delete => None,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&InternalKey, &Vec<u8>)> {
        self.table.iter()
    }

    pub fn apply(&mut self, record: WalRecord) {
        match record.kind {
            RecordKind::Put => self.put(record.key, record.value, record.sequence),
            RecordKind::Delete => self.delete(record.key, record.sequence),
        }
    }
}

impl Default for MemTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_latest_value() {
        let mut memtable = MemTable::new();

        memtable.put(b"name".to_vec(), b"Alice".to_vec(), 1);
        memtable.put(b"name".to_vec(), b"Bob".to_vec(), 2);

        assert_eq!(memtable.get(b"name", 2), Some(b"Bob".as_slice()));
    }

    #[test]
    fn get_respects_snapshot_sequence() {
        let mut memtable = MemTable::new();

        memtable.put(b"name".to_vec(), b"Alice".to_vec(), 1);
        memtable.put(b"name".to_vec(), b"Bob".to_vec(), 2);
        memtable.put(b"name".to_vec(), b"Charlie".to_vec(), 3);

        assert_eq!(memtable.get(b"name", 3), Some(b"Charlie".as_slice()));

        assert_eq!(memtable.get(b"name", 2), Some(b"Bob".as_slice()));

        assert_eq!(memtable.get(b"name", 1), Some(b"Alice".as_slice()));
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let mut memtable = MemTable::new();

        memtable.put(b"name".to_vec(), b"Alice".to_vec(), 1);

        assert_eq!(memtable.get(b"missing", 1), None);
    }

    #[test]
    fn delete_hides_value() {
        let mut memtable = MemTable::new();

        memtable.put(b"name".to_vec(), b"Alice".to_vec(), 1);
        memtable.delete(b"name".to_vec(), 2);

        assert_eq!(memtable.get(b"name", 2), None);
    }

    #[test]
    fn delete_does_not_affect_older_snapshot() {
        let mut memtable = MemTable::new();

        memtable.put(b"name".to_vec(), b"Alice".to_vec(), 1);
        memtable.delete(b"name".to_vec(), 2);

        assert_eq!(memtable.get(b"name", 1), Some(b"Alice".as_slice()));

        assert_eq!(memtable.get(b"name", 2), None);
    }

    #[test]
    fn delete_is_visible_only_from_its_sequence_onward() {
        let mut memtable = MemTable::new();

        memtable.put(b"name".to_vec(), b"Alice".to_vec(), 10);
        memtable.delete(b"name".to_vec(), 20);

        assert_eq!(memtable.get(b"name", 19), Some(b"Alice".as_slice()));

        assert_eq!(memtable.get(b"name", 20), None);
        assert_eq!(memtable.get(b"name", 100), None);
    }

    #[test]
    fn different_keys_are_independent() {
        let mut memtable = MemTable::new();

        memtable.put(b"a".to_vec(), b"one".to_vec(), 1);
        memtable.put(b"b".to_vec(), b"two".to_vec(), 2);
        memtable.delete(b"a".to_vec(), 3);

        assert_eq!(memtable.get(b"a", 3), None);
        assert_eq!(memtable.get(b"b", 3), Some(b"two".as_slice()));
    }

    #[test]
    fn older_version_is_found_when_newer_version_is_after_snapshot() {
        let mut memtable = MemTable::new();

        memtable.put(b"key".to_vec(), b"v1".to_vec(), 10);
        memtable.put(b"key".to_vec(), b"v2".to_vec(), 20);
        memtable.put(b"key".to_vec(), b"v3".to_vec(), 30);

        assert_eq!(memtable.get(b"key", 15), Some(b"v1".as_slice()));

        assert_eq!(memtable.get(b"key", 25), Some(b"v2".as_slice()));
    }

    #[test]
    fn empty_value_is_distinct_from_missing_key() {
        let mut memtable = MemTable::new();

        memtable.put(b"key".to_vec(), Vec::new(), 1);

        assert_eq!(memtable.get(b"key", 1), Some(&[] as &[u8]));

        assert_eq!(memtable.get(b"missing", 1), None);
    }

    #[test]
    fn entries_are_sorted_by_internal_key() {
        let mut memtable = MemTable::new();

        memtable.put(b"b".to_vec(), b"b1".to_vec(), 1);
        memtable.put(b"a".to_vec(), b"a1".to_vec(), 1);
        memtable.put(b"b".to_vec(), b"b2".to_vec(), 2);
        memtable.put(b"a".to_vec(), b"a2".to_vec(), 2);

        let entries: Vec<_> = memtable.iter().collect();

        assert_eq!(entries.len(), 4);

        assert_eq!(entries[0].0.user_key, b"a");
        assert_eq!(entries[0].0.sequence, 2);

        assert_eq!(entries[1].0.user_key, b"a");
        assert_eq!(entries[1].0.sequence, 1);

        assert_eq!(entries[2].0.user_key, b"b");
        assert_eq!(entries[2].0.sequence, 2);

        assert_eq!(entries[3].0.user_key, b"b");
        assert_eq!(entries[3].0.sequence, 1);
    }

    #[test]
    fn apply_put_record() {
        let mut memtable = MemTable::new();

        let record = WalRecord::put(1, b"hello".to_vec(), b"world".to_vec());

        memtable.apply(record);

        assert_eq!(memtable.get(b"hello", 1), Some(b"world".as_slice()));
    }

    #[test]
    fn apply_delete_record() {
        let mut memtable = MemTable::new();

        memtable.put(b"hello".to_vec(), b"world".to_vec(), 1);

        memtable.apply(WalRecord::delete(2, b"hello".to_vec()));

        assert_eq!(memtable.get(b"hello", 2), None);
    }

    #[test]
    fn replay_records_reconstructs_memtable() {
        let records = vec![
            WalRecord::put(1, b"name".to_vec(), b"Alice".to_vec()),
            WalRecord::put(2, b"city".to_vec(), b"Chennai".to_vec()),
            WalRecord::put(3, b"name".to_vec(), b"Bob".to_vec()),
            WalRecord::delete(4, b"city".to_vec()),
        ];

        let mut memtable = MemTable::new();

        for record in records {
            memtable.apply(record);
        }

        assert_eq!(memtable.get(b"name", 4), Some(b"Bob".as_slice()));

        assert_eq!(memtable.get(b"city", 4), None);

        assert_eq!(memtable.get(b"name", 2), Some(b"Alice".as_slice()));

        assert_eq!(memtable.get(b"city", 2), Some(b"Chennai".as_slice()));
    }
}
