//! Settings for the `wemake-python-styleguide` plugin.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "WemakePythonStyleguideOptions"
)]
pub struct Options {
    #[option(
        default = r#"2"#,
        value_type = "int",
        example = r#"
            min-name-length = 3
        "#
    )]
    /// Minimum name length for variables and methods.
    pub min_name_length: Option<usize>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub min_name_length: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self { min_name_length: 2 }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            min_name_length: options.min_name_length.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            min_name_length: Some(settings.min_name_length),
        }
    }
}
