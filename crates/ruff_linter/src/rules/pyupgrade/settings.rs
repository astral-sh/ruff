//! Settings for the `pyupgrade` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub keep_runtime_typing: bool,
}
