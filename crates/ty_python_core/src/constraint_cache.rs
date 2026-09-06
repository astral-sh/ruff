use std::hash::{BuildHasher, Hash};

use hashbrown::HashTable;
use hashbrown::hash_table::Entry;
use rustc_hash::FxBuildHasher;

const SHARDS: usize = 8;

#[derive(Debug)]
struct CachedOperation<T> {
    operands: (T, T),
    result: T,
}

/// Memoizes binary constraint operations without resizing one large hash table.
///
/// Growing a hash table temporarily keeps both its old and new allocations. Independent shards
/// limit that overlap for large constraint graphs. Empty caches do not allocate any shards.
#[derive(Debug)]
pub(crate) struct BinaryConstraintCache<T> {
    shards: Option<Box<[HashTable<CachedOperation<T>>; SHARDS]>>,
}

impl<T> Default for BinaryConstraintCache<T> {
    fn default() -> Self {
        Self { shards: None }
    }
}

impl<T: Copy + Eq + Hash> BinaryConstraintCache<T> {
    fn shard(hash: u64) -> usize {
        usize::from(hash.to_le_bytes()[4]) % SHARDS
    }

    pub(crate) fn get(&self, operands: &(T, T)) -> Option<&T> {
        let shards = self.shards.as_ref()?;
        let hash = FxBuildHasher.hash_one(operands);
        shards[Self::shard(hash)]
            .find(hash, |entry| entry.operands == *operands)
            .map(|entry| &entry.result)
    }

    pub(crate) fn insert(&mut self, operands: (T, T), result: T) {
        let shards = self
            .shards
            .get_or_insert_with(|| Box::new(std::array::from_fn(|_| HashTable::new())));
        let hash = FxBuildHasher.hash_one(operands);
        match shards[Self::shard(hash)].entry(
            hash,
            |entry| entry.operands == operands,
            |entry| FxBuildHasher.hash_one(entry.operands),
        ) {
            Entry::Occupied(mut entry) => entry.get_mut().result = result,
            Entry::Vacant(entry) => {
                entry.insert(CachedOperation { operands, result });
            }
        }
    }
}
