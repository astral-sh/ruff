use std::path::{Path, PathBuf};

pub use cache_key::{CacheKey, CacheKeyHasher};

mod cache_key;
pub mod filetime;
pub mod globset;

pub const CACHE_DIR_NAME: &str = ".ruff_cache";

/// Return the cache directory for a given project root.
pub fn cache_dir(overwrite: Option<PathBuf>, project_root: &Path) -> PathBuf {
    match overwrite {
        // User defined directory.
        Some(overwrite) => overwrite,
        // Default to the global directory.
        None => match dirs::cache_dir() {
            Some(mut path) => {
                path.push("ruff");
                path
            }
            // Falling back to a directory in the project.
            None => project_root.join(CACHE_DIR_NAME),
        },
    }
}
