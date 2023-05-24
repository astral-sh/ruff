use globset::{Glob, GlobMatcher};

use crate::{CacheKey, CacheKeyHasher};

impl CacheKey for GlobMatcher {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.glob().cache_key(state);
    }
}

impl CacheKey for Glob {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.glob().cache_key(state);
    }
}
