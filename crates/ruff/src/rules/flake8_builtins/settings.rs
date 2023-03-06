//! Settings for the `flake8-builtins` plugin.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8BuiltinsOptions"
)]
pub struct Options {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = "builtins-ignorelist = [\"id\"]"
    )]
    /// Ignore list of builtins.
    pub builtins_ignorelist: Option<Vec<String>>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub builtins_ignorelist: Vec<String>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            builtins_ignorelist: options.builtins_ignorelist.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            builtins_ignorelist: Some(settings.builtins_ignorelist),
        }
    }
}
