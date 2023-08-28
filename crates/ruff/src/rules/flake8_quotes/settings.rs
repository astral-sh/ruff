//! Settings for the `flake8-quotes` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Quote {
    /// Use double quotes.
    Double,
    /// Use single quotes.
    Single,
}

impl Default for Quote {
    fn default() -> Self {
        Self::Double
    }
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub inline_quotes: Quote,
    pub multiline_quotes: Quote,
    pub docstring_quotes: Quote,
    pub avoid_escape: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inline_quotes: Quote::default(),
            multiline_quotes: Quote::default(),
            docstring_quotes: Quote::default(),
            avoid_escape: true,
        }
    }
}
