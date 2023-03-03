use crate::{CacheKey, CacheKeyHasher};
use filetime::FileTime;
use std::hash::Hash;

impl CacheKey for FileTime {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.hash(&mut **state);
    }
}
