//! Settings for the `flake_quotes` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Quote {
    Single,
    Double,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub inline_quotes: Option<Quote>,
    pub multiline_quotes: Option<Quote>,
    pub docstring_quotes: Option<Quote>,
    pub avoid_escape: Option<bool>,
}

#[derive(Debug)]
pub struct Settings {
    pub inline_quotes: Quote,
    pub multiline_quotes: Quote,
    pub docstring_quotes: Quote,
    pub avoid_escape: bool,
}

impl Settings {
    pub fn from_config(config: Config) -> Self {
        Self {
            inline_quotes: config.inline_quotes.unwrap_or(Quote::Single),
            multiline_quotes: config.multiline_quotes.unwrap_or(Quote::Double),
            docstring_quotes: config.docstring_quotes.unwrap_or(Quote::Double),
            avoid_escape: config.avoid_escape.unwrap_or(true),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inline_quotes: Quote::Single,
            multiline_quotes: Quote::Double,
            docstring_quotes: Quote::Double,
            avoid_escape: true,
        }
    }
}
