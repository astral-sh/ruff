//! Settings for the `mccabe` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "McCabeOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = "10",
        value_type = "int",
        example = r#"
            # Flag errors (`C901`) whenever the complexity level exceeds 5.
            max-complexity = 5
        "#
    )]
    /// The maximum McCabe complexity to allow before triggering `C901` errors.
    pub max_complexity: Option<usize>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub max_complexity: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { max_complexity: 10 }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            max_complexity: options.max_complexity.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            max_complexity: Some(settings.max_complexity),
        }
    }
}
