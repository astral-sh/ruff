//! Settings for the `flake8-type-checking` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8TypeCheckingOptions"
)]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = r#"
            strict = true
        "#
    )]
    /// Enforce TC001, TC002, and TC003 rules even when valid runtime imports
    /// are present for the same module.
    /// See: https://github.com/snok/flake8-type-checking#strict.
    pub strict: Option<bool>,
}

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub strict: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            strict: options.strict.unwrap_or_default(),
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
