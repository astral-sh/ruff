//! Settings for the `mccabe` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub max_complexity: usize,
}

pub const DEFAULT_MAX_COMPLEXITY: usize = 10;

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_complexity: DEFAULT_MAX_COMPLEXITY,
        }
    }
}
