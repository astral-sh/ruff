//! Settings for the `flake8-errmsg` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8ErrMsgOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(default = "0", value_type = "int", example = "max-string-length = 20")]
    /// Maximum string length for string literals in exception messages.
    pub max_string_length: Option<usize>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub max_string_length: usize,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            max_string_length: options.max_string_length.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            max_string_length: Some(settings.max_string_length),
        }
    }
}
