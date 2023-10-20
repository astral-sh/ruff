//! Settings for the `Pyflakes` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub extend_generics: Vec<String>,
}
