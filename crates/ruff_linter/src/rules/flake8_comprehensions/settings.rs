//! Settings for the `flake8-comprehensions` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub allow_dict_calls_with_keyword_arguments: bool,
}
