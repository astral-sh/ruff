//! Settings for the `flake8-quotes` plugin.

use ruff_macros::ConfigurationOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Quote {
    Single,
    Double,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = r#"
            Quote style to prefer for inline strings (either "single" (`'`) or "double" (`"`)).
        "#,
        default = r#""double""#,
        value_type = "Quote",
        example = r#"
            inline-quotes = "single"
        "#
    )]
    pub inline_quotes: Option<Quote>,
    #[option(
        doc = r#"
            Quote style to prefer for multiline strings (either "single" (`'`) or "double" (`"`)).
        "#,
        default = r#""double""#,
        value_type = "Quote",
        example = r#"
            multiline-quotes = "single"
        "#
    )]
    pub multiline_quotes: Option<Quote>,
    #[option(
        doc = r#"
            Quote style to prefer for docstrings (either "single" (`'`) or "double" (`"`)).
        "#,
        default = r#""double""#,
        value_type = "Quote",
        example = r#"
            docstring-quotes = "single"
        "#
    )]
    pub docstring_quotes: Option<Quote>,
    #[option(
        doc = r#"
            Whether to avoid using single quotes if a string contains single quotes, or vice-versa
            with double quotes, as per [PEP8](https://peps.python.org/pep-0008/#string-quotes).
            This minimizes the need to escape quotation marks within strings.
        "#,
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            # Don't bother trying to avoid escapes.
            avoid-escape = false
        "#
    )]
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
