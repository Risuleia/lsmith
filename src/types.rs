use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    Put,
    Delete,
}

impl RecordKind {
    pub(crate) fn sort_order(self) -> u8 {
        match self {
            Self::Put => 0,
            Self::Delete => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalKey {
    pub user_key: Vec<u8>,
    pub sequence: u64,
    pub kind: RecordKind,
}

impl InternalKey {
    pub fn new(user_key: Vec<u8>, sequence: u64, kind: RecordKind) -> Self {
        Self { user_key, sequence, kind }
    }

    pub fn put(user_key: Vec<u8>, sequence: u64) -> Self {
        Self::new(user_key, sequence, RecordKind::Put)
    }

    pub fn delete(user_key: Vec<u8>, sequence: u64) -> Self {
        Self::new(user_key, sequence, RecordKind::Delete)
    }
}

impl Ord for InternalKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.user_key.cmp(&other.user_key) {
            Ordering::Equal => match other.sequence.cmp(&self.sequence) {
                Ordering::Equal => self.kind.sort_order().cmp(&other.kind.sort_order()),

                ordering => ordering,
            },

            ordering => ordering,
        }
    }
}

impl PartialOrd for InternalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_creates_put_record() {
        let key = InternalKey::put(b"hello".to_vec(), 42);

        assert_eq!(key.user_key, b"hello");
        assert_eq!(key.sequence, 42);
        assert_eq!(key.kind, RecordKind::Put);
    }

    #[test]
    fn delete_creates_delete_record() {
        let key = InternalKey::delete(b"hello".to_vec(), 42);

        assert_eq!(key.user_key, b"hello");
        assert_eq!(key.sequence, 42);
        assert_eq!(key.kind, RecordKind::Delete);
    }

    #[test]
    fn user_keys_sort_ascending() {
        let a = InternalKey::put(b"apple".to_vec(), 1);
        let b = InternalKey::put(b"banana".to_vec(), 1);
        let c = InternalKey::put(b"cherry".to_vec(), 1);

        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn sequence_numbers_sort_descending() {
        let old = InternalKey::put(b"key".to_vec(), 10);
        let new = InternalKey::put(b"key".to_vec(), 20);

        assert!(new < old);
    }

    #[test]
    fn versions_of_same_key_sort_newest_first() {
        let v1 = InternalKey::put(b"key".to_vec(), 1);
        let v2 = InternalKey::put(b"key".to_vec(), 2);
        let v3 = InternalKey::put(b"key".to_vec(), 3);

        let mut keys = vec![v1, v3, v2];
        keys.sort();

        assert_eq!(keys[0].sequence, 3);
        assert_eq!(keys[1].sequence, 2);
        assert_eq!(keys[2].sequence, 1);
    }

    #[test]
    fn delete_and_put_with_same_sequence_have_deterministic_order() {
        let put = InternalKey::put(b"key".to_vec(), 10);
        let delete = InternalKey::delete(b"key".to_vec(), 10);

        assert_ne!(put, delete);

        // The exact ordering is determined by RecordKind::sort_order().
        assert!(put < delete);
    }

    #[test]
    fn complete_ordering() {
        let mut keys = vec![
            InternalKey::put(b"b".to_vec(), 1),
            InternalKey::put(b"a".to_vec(), 5),
            InternalKey::put(b"a".to_vec(), 10),
            InternalKey::put(b"b".to_vec(), 20),
            InternalKey::put(b"a".to_vec(), 3),
        ];

        keys.sort();

        assert_eq!(keys[0].user_key, b"a");
        assert_eq!(keys[0].sequence, 10);

        assert_eq!(keys[1].user_key, b"a");
        assert_eq!(keys[1].sequence, 5);

        assert_eq!(keys[2].user_key, b"a");
        assert_eq!(keys[2].sequence, 3);

        assert_eq!(keys[3].user_key, b"b");
        assert_eq!(keys[3].sequence, 20);

        assert_eq!(keys[4].user_key, b"b");
        assert_eq!(keys[4].sequence, 1);
    }
}
