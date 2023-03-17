//! Settings for the `flake8-self` plugin.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

// By default, ignore the `namedtuple` methods and attributes, which are underscore-prefixed to
// prevent conflicts with field names.
const IGNORE_NAMES: [&str; 5] = ["_make", "_asdict", "_replace", "_fields", "_field_defaults"];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8SelfOptions"
)]
pub struct Options {
    #[option(
        default = r#"["_make", "_asdict", "_replace", "_fields", "_field_defaults"]"#,
        value_type = "list[str]",
        example = r#"
            ignore-names = ["_new"]
        "#
    )]
    /// A list of names to ignore when considering `flake8-self` violations.
    pub ignore_names: Option<Vec<String>>,
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub ignore_names: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES.map(String::from).to_vec(),
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ignore_names: options
                .ignore_names
                .unwrap_or_else(|| IGNORE_NAMES.map(String::from).to_vec()),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ignore_names: Some(settings.ignore_names),
        }
    }
}
