//! Settings for the `flake8-variables-names` plugin.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8VariablesNamesOptions"
)]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = "use-varnames-strict-mode = true"
    )]
    /// Whether to use strict settings for the allow/block lists.
    /// Strict mode allows `_` or `T` for single char variables
    /// and increases the blocklist for vague/non-descript variable names
    /// such as `results`, `data`, `info`, etc.
    pub strict: Option<bool>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub strict: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            strict: options.strict.unwrap_or(false),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            strict: Some(settings.strict),
        }
    }
}
