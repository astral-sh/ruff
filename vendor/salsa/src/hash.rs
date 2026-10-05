use std::hash::{BuildHasher, Hash, Hasher};

pub(crate) type FxHasher = std::hash::BuildHasherDefault<rustc_hash::FxHasher>;
pub(crate) type FxIndexSet<K> = indexmap::IndexSet<K, FxHasher>;
pub(crate) type FxLinkedHashSet<K> = hashlink::LinkedHashSet<K, FxHasher>;
pub(crate) type FxHashSet<K> = std::collections::HashSet<K, FxHasher>;

pub(crate) fn hash<T: Hash>(t: &T) -> u64 {
    FxHasher::default().hash_one(t)
}

// `TypeId` is a 128-bit hash internally, and it's `Hash` implementation
// writes the lower 64-bits. Hashing it again would be unnecessary.
#[derive(Default)]
pub(crate) struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    fn write(&mut self, _: &[u8]) {
        unreachable!("`TypeId` calls `write_u64`");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}
