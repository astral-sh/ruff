//! Settings for the `flake8-quotes` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Quote {
    /// Use single quotes.
    Single,
    /// Use double quotes.
    Double,
}

impl Default for Quote {
    fn default() -> Self {
        Self::Double
    }
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8QuotesOptions"
)]
pub struct Options {
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            inline-quotes = "single"
        "#
    )]
    /// Quote style to prefer for inline strings (either "single" or
    /// "double").
    pub inline_quotes: Option<Quote>,
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            multiline-quotes = "single"
        "#
    )]
    /// Quote style to prefer for multiline strings (either "single" or
    /// "double").
    pub multiline_quotes: Option<Quote>,
    #[option(
        default = r#""double""#,
        value_type = r#""single" | "double""#,
        example = r#"
            docstring-quotes = "single"
        "#
    )]
    /// Quote style to prefer for docstrings (either "single" or "double").
    pub docstring_quotes: Option<Quote>,
    #[option(
        default = r#"true"#,
        value_type = "bool",
        example = r#"
            # Don't bother trying to avoid escapes.
            avoid-escape = false
        "#
    )]
    /// Whether to avoid using single quotes if a string contains single quotes,
    /// or vice-versa with double quotes, as per [PEP8](https://peps.python.org/pep-0008/#string-quotes).
    /// This minimizes the need to escape quotation marks within strings.
    pub avoid_escape: Option<bool>,
}

#[derive(Debug, Hash)]
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

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            inline_quotes: options.inline_quotes.unwrap_or_default(),
            multiline_quotes: options.multiline_quotes.unwrap_or_default(),
            docstring_quotes: options.docstring_quotes.unwrap_or_default(),
            avoid_escape: options.avoid_escape.unwrap_or(true),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            inline_quotes: Some(settings.inline_quotes),
            multiline_quotes: Some(settings.multiline_quotes),
            docstring_quotes: Some(settings.docstring_quotes),
            avoid_escape: Some(settings.avoid_escape),
        }
    }
}
