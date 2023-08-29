//! Settings for the `flake8-bugbear` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub extend_immutable_calls: Vec<String>,
}
