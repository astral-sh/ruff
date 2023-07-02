//! Settings for the `wemake-python-styleguide` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, CacheKey)]
pub struct Settings {
    /// The minimum number of alphanumeric characters in a name.
    pub min_name_length: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { min_name_length: 2 }
    }
}
