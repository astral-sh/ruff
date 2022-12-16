//! Settings for the `flake8-errmsg` plugin.

use ruff_macros::ConfigurationOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = r#"
            Maximum string length for string literals in exception messages.
        "#,
        default = "0",
        value_type = "usize",
        example = "max-string-length = 20"
    )]
    pub max_string_length: Option<usize>,
}

#[derive(Debug, Default, Hash)]
pub struct Settings {
    pub max_string_length: usize,
}

impl Settings {
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_options(options: Options) -> Self {
        Self {
            max_string_length: options.max_string_length.unwrap_or_default(),
        }
    }
}
