//! Settings for the `flake8-implicit-str-concat` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub allow_multiline: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_multiline: true,
        }
    }
}
