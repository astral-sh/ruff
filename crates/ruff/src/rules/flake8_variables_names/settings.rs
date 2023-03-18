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
    /// Whether to use strict settings for the allow/block lists
    pub use_varnames_strict_mode: Option<bool>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub use_varnames_strict_mode: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            use_varnames_strict_mode: options.use_varnames_strict_mode.unwrap_or(false),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            use_varnames_strict_mode: Some(settings.use_varnames_strict_mode),
        }
    }
}
