//! Settings for the `flake8-comprehensions` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8ComprehensionsOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = "allow-dict-calls-with-keyword-arguments = true"
    )]
    /// Allow `dict` calls that make use of keyword arguments (e.g., `dict(a=1, b=2)`).
    pub allow_dict_calls_with_keyword_arguments: Option<bool>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub allow_dict_calls_with_keyword_arguments: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            allow_dict_calls_with_keyword_arguments: options
                .allow_dict_calls_with_keyword_arguments
                .unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            allow_dict_calls_with_keyword_arguments: Some(
                settings.allow_dict_calls_with_keyword_arguments,
            ),
        }
    }
}
