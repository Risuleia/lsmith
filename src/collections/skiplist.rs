use std::{alloc::Layout, cmp::Ordering, marker::PhantomData, ptr::NonNull};

const MAX_HEIGHT: usize = 20;
const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

const INITIAL_SEED: u64 = 0x1234_5678_9ABC_DEF0;
const SEED_MULTIPLICANT: u64 = 0x2545_F491_4F6C_DD1D;

type Link<K, V> = Option<NonNull<Node<K, V>>>;

struct Node<K, V> {
    key: K,
    value: V,
    next: [Link<K, V>; MAX_HEIGHT],
}

impl<K, V> Node<K, V> {
    fn new(key: K, value: V) -> Self {
        Self { key, value, next: [None; MAX_HEIGHT] }
    }
}

struct Head<K, V> {
    next: [Link<K, V>; MAX_HEIGHT],
}

impl<K, V> Head<K, V> {
    fn new() -> Self {
        Self { next: [None; MAX_HEIGHT] }
    }
}

struct Chunk {
    ptr: NonNull<u8>,
    layout: Layout,
    used: usize,
}

impl Chunk {
    fn new(size: usize) -> Self {
        let layout = Layout::array::<u8>(size).expect("invalid arena chunk layout");

        let ptr = unsafe { std::alloc::alloc(layout) };

        let ptr = NonNull::new(ptr).unwrap_or_else(|| std::alloc::handle_alloc_error(layout));

        Self { ptr, layout, used: 0 }
    }

    fn remaining(&self) -> usize {
        self.layout.size() - self.used
    }

    fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let base = self.ptr.as_ptr() as usize;

        let current = base + self.used;

        let aligned = (current + layout.align() - 1) & !(layout.align() - 1);

        let padding = aligned - current;

        let new_used = self.used.checked_add(padding)?.checked_add(layout.size())?;

        if new_used > self.layout.size() {
            return None;
        }

        self.used = new_used;

        Some(unsafe { NonNull::new_unchecked(aligned as *mut u8) })
    }
}

impl Drop for Chunk {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(self.ptr.as_ptr(), self.layout);
        }
    }
}

struct Arena {
    chunks: Vec<Chunk>,
    chunk_size: usize,
}

impl Arena {
    fn new(chunk_size: usize) -> Self {
        Self { chunks: Vec::new(), chunk_size }
    }

    fn allocate<T>(&mut self, value: T) -> NonNull<T> {
        let layout = Layout::new::<T>();

        if self.chunks.is_empty() {
            self.chunks.push(Chunk::new(self.chunk_size.max(layout.size() + layout.align())));
        }

        let index = self.chunks.len() - 1;

        if let Some(ptr) = self.chunks[index].allocate(layout) {
            unsafe {
                ptr.cast::<T>().as_ptr().write(value);
                return ptr.cast();
            }
        }

        let chunk_size = self.chunk_size.max(layout.size() + layout.align());

        self.chunks.push(Chunk::new(chunk_size));

        let chunk = self.chunks.last_mut().unwrap();

        let ptr = chunk.allocate(layout).expect("new arena chunk must have enough space");

        unsafe {
            ptr.cast::<T>().as_ptr().write(value);
        }

        ptr.cast()
    }
}

pub struct SkipList<K, V> {
    arena: Arena,
    head: Head<K, V>,
    height: usize,
    len: usize,
    seed: u64,
}

impl<K: Ord, V> SkipList<K, V> {
    pub fn new() -> Self {
        Self::with_chunk_size(DEFAULT_CHUNK_SIZE)
    }

    pub fn with_chunk_size(chunk_size: usize) -> Self {
        assert!(chunk_size > 0);

        Self {
            arena: Arena::new(chunk_size),
            head: Head::new(),
            height: 1,
            len: 0,
            seed: INITIAL_SEED,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let node = self.find_node(key)?;

        unsafe { Some(&(*node.as_ptr()).value) }
    }

    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut update: [Link<K, V>; MAX_HEIGHT] = [None; MAX_HEIGHT];

        let mut current: Option<NonNull<Node<K, V>>> = None;

        for level in (0..self.height).rev() {
            loop {
                let next = match current {
                    Some(node) => unsafe { (*node.as_ptr()).next[level] },

                    None => self.head.next[level],
                };

                match next {
                    Some(node) => match unsafe { (*node.as_ptr()).key.cmp(&key) } {
                        Ordering::Less => {
                            current = Some(node);
                        }

                        Ordering::Equal => {
                            return Some(unsafe {
                                std::ptr::replace(&mut (*node.as_ptr()).value, value)
                            });
                        }

                        Ordering::Greater => break,
                    },

                    None => break,
                }
            }

            update[level] = current;
        }

        let node_height = self.random_height();

        if node_height > self.height {
            for level in self.height..node_height {
                update[level] = None;
            }

            self.height = node_height;
        }

        let node = self.arena.allocate(Node::new(key, value));

        for level in 0..node_height {
            let next = match update[level] {
                Some(previous) => unsafe { (*previous.as_ptr()).next[level] },

                None => self.head.next[level],
            };

            unsafe {
                (*node.as_ptr()).next[level] = next;
            }

            match update[level] {
                Some(previous) => unsafe {
                    (*previous.as_ptr()).next[level] = Some(node);
                },

                None => {
                    self.head.next[level] = Some(node);
                }
            }
        }

        self.len += 1;

        None
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter { current: self.head.next[0], marker: PhantomData }
    }

    pub fn lower_bound(&self, key: &K) -> Option<(&K, &V)> {
        let mut current: Option<NonNull<Node<K, V>>> = None;

        for level in (0..self.height).rev() {
            loop {
                let next = match current {
                    Some(node) => unsafe { (*node.as_ptr()).next[level] },
                    None => self.head.next[level],
                };

                match next {
                    Some(node) => match unsafe { (*node.as_ptr()).key.cmp(key) } {
                        Ordering::Less => {
                            current = Some(node);
                        }

                        Ordering::Equal | Ordering::Greater => break,
                    },
                    None => break,
                }
            }
        }

        let node = match current {
            Some(node) => unsafe { (*node.as_ptr()).next[0] },
            None => self.head.next[0],
        }?;

        unsafe { Some((&(*node.as_ptr()).key, &(*node.as_ptr()).value)) }
    }

    fn find_node(&self, key: &K) -> Option<NonNull<Node<K, V>>> {
        let mut current: Option<NonNull<Node<K, V>>> = None;

        for level in (0..self.height).rev() {
            loop {
                let next = match current {
                    Some(node) => unsafe { (*node.as_ptr()).next[level] },

                    None => self.head.next[level],
                };

                match next {
                    Some(node) => match unsafe { (*node.as_ptr()).key.cmp(key) } {
                        Ordering::Less => {
                            current = Some(node);
                        }

                        Ordering::Equal => {
                            return Some(node);
                        }

                        Ordering::Greater => break,
                    },

                    None => break,
                }
            }
        }

        None
    }

    fn random_height(&mut self) -> usize {
        let mut height = 1;

        while height < MAX_HEIGHT {
            if self.next_random() & 1 == 0 {
                break;
            }

            height += 1;
        }

        height
    }

    fn next_random(&mut self) -> u64 {
        let mut x = self.seed;

        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;

        self.seed = x;

        x.wrapping_mul(SEED_MULTIPLICANT)
    }
}

impl<K: Ord, V> Default for SkipList<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> Drop for SkipList<K, V> {
    fn drop(&mut self) {
        let mut current = self.head.next[0];

        while let Some(node) = current {
            unsafe {
                current = (*node.as_ptr()).next[0];
                std::ptr::drop_in_place(node.as_ptr());
            }
        }
    }
}

pub struct Iter<'a, K, V> {
    current: Link<K, V>,
    marker: PhantomData<&'a SkipList<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.current?;

        unsafe {
            let node_ref = node.as_ref();
            self.current = node_ref.next[0];

            Some((&node_ref.key, &node_ref.value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SkipList;

    #[test]
    fn new_list_is_empty() {
        let list: SkipList<i32, i32> = SkipList::new();

        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
    }

    #[test]
    fn insert_and_get() {
        let mut list = SkipList::new();

        assert_eq!(list.insert(10, "ten"), None);
        assert_eq!(list.len(), 1);

        assert_eq!(list.get(&10), Some(&"ten"));
    }

    #[test]
    fn get_missing_key() {
        let mut list = SkipList::new();

        list.insert(10, "ten");

        assert_eq!(list.get(&5), None);
        assert_eq!(list.get(&20), None);
    }

    #[test]
    fn contains() {
        let mut list = SkipList::new();

        list.insert(10, "ten");

        assert!(list.contains(&10));
        assert!(!list.contains(&5));
    }

    #[test]
    fn insert_replaces_existing_value() {
        let mut list = SkipList::new();

        assert_eq!(list.insert(10, "old"), None);
        assert_eq!(list.len(), 1);

        assert_eq!(list.insert(10, "new"), Some("old"));
        assert_eq!(list.len(), 1);

        assert_eq!(list.get(&10), Some(&"new"));
    }

    #[test]
    fn keys_are_sorted() {
        let mut list = SkipList::new();

        list.insert(50, "fifty");
        list.insert(10, "ten");
        list.insert(30, "thirty");
        list.insert(20, "twenty");
        list.insert(40, "forty");

        let keys: Vec<_> = list.iter().map(|(key, _)| *key).collect();

        assert_eq!(keys, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn values_follow_sorted_keys() {
        let mut list = SkipList::new();

        list.insert(3, "three");
        list.insert(1, "one");
        list.insert(2, "two");

        let entries: Vec<_> = list.iter().collect();

        assert_eq!(entries, vec![(&1, &"one"), (&2, &"two"), (&3, &"three"),]);
    }

    #[test]
    fn iteration_empty_list() {
        let list: SkipList<i32, i32> = SkipList::new();

        assert_eq!(list.iter().next(), None);
    }

    #[test]
    fn iteration_single_element() {
        let mut list = SkipList::new();

        list.insert(42, 100);

        let mut iter = list.iter();

        assert_eq!(iter.next(), Some((&42, &100)));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn iteration_does_not_duplicate_elements() {
        let mut list = SkipList::new();

        for i in 0..100 {
            list.insert(i, i * 10);
        }

        let entries: Vec<_> = list.iter().collect();

        assert_eq!(entries.len(), 100);

        for i in 0..100 {
            assert_eq!(entries[i], (&i, &(i * 10)));
        }
    }

    #[test]
    fn duplicate_inserts_do_not_increase_length() {
        let mut list = SkipList::new();

        for _ in 0..100 {
            list.insert(42, "value");
        }

        assert_eq!(list.len(), 1);
        assert_eq!(list.get(&42), Some(&"value"));
    }

    #[test]
    fn many_elements() {
        let mut list = SkipList::new();

        for i in 0..10_000 {
            list.insert(i, i * 2);
        }

        assert_eq!(list.len(), 10_000);

        for i in 0..10_000 {
            assert_eq!(list.get(&i), Some(&(i * 2)));
        }
    }

    #[test]
    fn reverse_insertion() {
        let mut list = SkipList::new();

        for i in (0..10_000).rev() {
            list.insert(i, i);
        }

        assert_eq!(list.len(), 10_000);

        let keys: Vec<_> = list.iter().map(|(key, _)| *key).collect();

        assert_eq!(keys.len(), 10_000);

        for (expected, actual) in (0..10_000).zip(keys) {
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn random_insertion() {
        let mut list = SkipList::new();

        // Deterministic pseudo-random permutation.
        let mut values: Vec<u64> = (0..10_000).collect();

        let mut seed = 0x1234_5678_9ABC_DEF0u64;

        for i in (1..values.len()).rev() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;

            let j = (seed as usize) % (i + 1);

            values.swap(i, j);
        }

        for &value in &values {
            list.insert(value, value * 2);
        }

        assert_eq!(list.len(), 10_000);

        let keys: Vec<_> = list.iter().map(|(key, _)| *key).collect();

        for (expected, actual) in (0..10_000u64).zip(keys) {
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn tiny_arena_forces_many_chunks() {
        let mut list = SkipList::with_chunk_size(64);

        for i in 0..1_000 {
            list.insert(i, i * 10);
        }

        assert_eq!(list.len(), 1_000);

        for i in 0..1_000 {
            assert_eq!(list.get(&i), Some(&(i * 10)));
        }
    }

    #[test]
    fn large_values() {
        let mut list = SkipList::new();

        let value = vec![0xAB; 1024 * 1024];

        list.insert(1, value.clone());

        assert_eq!(list.get(&1), Some(&value));
    }

    #[test]
    fn empty_key_and_value() {
        let mut list = SkipList::new();

        list.insert(Vec::<u8>::new(), Vec::<u8>::new());

        assert!(list.contains(&Vec::<u8>::new()));
        assert_eq!(list.get(&Vec::<u8>::new()), Some(&Vec::<u8>::new()));
    }

    #[test]
    fn string_keys() {
        let mut list = SkipList::new();

        list.insert("banana", 2);
        list.insert("apple", 1);
        list.insert("cherry", 3);

        let entries: Vec<_> = list.iter().collect();

        assert_eq!(entries, vec![(&"apple", &1), (&"banana", &2), (&"cherry", &3),]);
    }

    #[test]
    fn arena_survives_many_allocations() {
        let mut list = SkipList::with_chunk_size(128);

        for i in 0..50_000 {
            list.insert(i, i);
        }

        for i in 0..50_000 {
            assert_eq!(list.get(&i), Some(&i));
        }

        assert_eq!(list.len(), 50_000);
    }

    #[test]
    fn values_are_dropped() {
        use std::cell::Cell;
        use std::rc::Rc;

        struct DropCounter {
            counter: Rc<Cell<usize>>,
        }

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.counter.set(self.counter.get() + 1);
            }
        }

        let counter = Rc::new(Cell::new(0));

        {
            let mut list = SkipList::new();

            for i in 0..100 {
                list.insert(i, DropCounter { counter: Rc::clone(&counter) });
            }

            assert_eq!(counter.get(), 0);
        }

        assert_eq!(counter.get(), 100);
    }

    #[test]
    fn lower_bound_finds_first_key_at_or_after_target() {
        let mut list = SkipList::new();

        list.insert(10, "ten");
        list.insert(20, "twenty");
        list.insert(30, "thirty");
        list.insert(40, "forty");
        list.insert(50, "fifty");

        assert_eq!(list.lower_bound(&5), Some((&10, &"ten")));
        assert_eq!(list.lower_bound(&10), Some((&10, &"ten")));
        assert_eq!(list.lower_bound(&15), Some((&20, &"twenty")));
        assert_eq!(list.lower_bound(&30), Some((&30, &"thirty")));
        assert_eq!(list.lower_bound(&45), Some((&50, &"fifty")));
        assert_eq!(list.lower_bound(&60), None);
    }

    #[test]
    fn lower_bound_on_empty_list() {
        let list: SkipList<i32, i32> = SkipList::new();

        assert_eq!(list.lower_bound(&10), None);
    }

    #[test]
    fn lower_bound_before_first_key() {
        let mut list = SkipList::new();

        list.insert(100, "hundred");
        list.insert(200, "two hundred");

        assert_eq!(list.lower_bound(&50), Some((&100, &"hundred")));
    }

    #[test]
    fn lower_bound_after_last_key() {
        let mut list = SkipList::new();

        list.insert(100, "hundred");
        list.insert(200, "two hundred");

        assert_eq!(list.lower_bound(&300), None);
    }
}
