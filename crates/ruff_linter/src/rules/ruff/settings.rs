//! Settings for the `ruff` rule set

use ruff_macros::CacheKey;

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub prefer_parentheses_getitem_tuple: bool,
}
