use std::path::{Path, PathBuf};

pub use cache_key::{CacheKey, CacheKeyHasher};

mod cache_key;
pub mod filetime;
pub mod globset;

pub const CACHE_DIR_NAME: &str = ".ruff_cache";

/// Return the cache directory for a given project root.
pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(CACHE_DIR_NAME)
}
