//! Settings for the `flake8-errmsg` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub max_string_length: usize,
}
