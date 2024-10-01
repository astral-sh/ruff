//! Settings for the `flake8-quotes` plugin.

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::display_settings;
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

impl From<ruff_python_ast::str::Quote> for Quote {
    fn from(value: ruff_python_ast::str::Quote) -> Self {
        match value {
            ruff_python_ast::str::Quote::Double => Self::Double,
            ruff_python_ast::str::Quote::Single => Self::Single,
        }
    }
}

#[derive(Debug, Clone, CacheKey)]
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

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_quotes",
            fields = [
                self.inline_quotes,
                self.multiline_quotes,
                self.docstring_quotes,
                self.avoid_escape
            ]
        }
        Ok(())
    }
}

impl Quote {
    #[must_use]
    pub const fn opposite(self) -> Self {
        match self {
            Self::Double => Self::Single,
            Self::Single => Self::Double,
        }
    }

    /// Get the character used to represent this quote.
    pub const fn as_char(self) -> char {
        match self {
            Self::Double => '"',
            Self::Single => '\'',
        }
    }
}

impl Display for Quote {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Double => write!(f, "double"),
            Self::Single => write!(f, "single"),
        }
    }
}
