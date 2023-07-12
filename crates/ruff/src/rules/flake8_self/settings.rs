//! Settings for the `flake8-self` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

// By default, ignore the `namedtuple` methods and attributes, as well as the
// _sunder_ names in Enum, which are underscore-prefixed to prevent conflicts
// with field names.
const IGNORE_NAMES: [&str; 7] = [
    "_make",
    "_asdict",
    "_replace",
    "_fields",
    "_field_defaults",
    "_name_",
    "_value_",
];

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8SelfOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#"["_make", "_asdict", "_replace", "_fields", "_field_defaults", "_name_", "_value_"]"#,
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
