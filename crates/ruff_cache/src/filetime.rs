use std::hash::Hash;

use filetime::FileTime;

use crate::{CacheKey, CacheKeyHasher};

impl CacheKey for FileTime {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.hash(&mut *state);
    }
}
