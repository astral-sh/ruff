//! Settings for the `flake8-quotes` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Quote {
    Single,
    Double,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub inline_quotes: Option<Quote>,
    pub multiline_quotes: Option<Quote>,
    pub docstring_quotes: Option<Quote>,
    pub avoid_escape: Option<bool>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub inline_quotes: Quote,
    pub multiline_quotes: Quote,
    pub docstring_quotes: Quote,
    pub avoid_escape: bool,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            inline_quotes: options.inline_quotes.unwrap_or(Quote::Double),
            multiline_quotes: options.multiline_quotes.unwrap_or(Quote::Double),
            docstring_quotes: options.docstring_quotes.unwrap_or(Quote::Double),
            avoid_escape: options.avoid_escape.unwrap_or(true),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inline_quotes: Quote::Double,
            multiline_quotes: Quote::Double,
            docstring_quotes: Quote::Double,
            avoid_escape: true,
        }
    }
}
