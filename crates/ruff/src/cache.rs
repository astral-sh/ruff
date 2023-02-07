use std::path::{Path, PathBuf};

pub const CACHE_DIR_NAME: &str = ".ruff_cache";

/// Return the cache directory for a given project root. Defers to the
/// `RUFF_CACHE_DIR` environment variable, if set.
pub fn cache_dir(project_root: &Path) -> PathBuf {
    project_root.join(CACHE_DIR_NAME)
}
