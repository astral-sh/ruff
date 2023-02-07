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
    /// See flake8-type-checking's [strict](https://github.com/snok/flake8-type-checking#strict) option.
    pub strict: Option<bool>,
    #[option(
        default = "[\"typing\"]",
        value_type = "list[str]",
        example = r#"
            exempt-modules = ["typing", "typing_extensions"]
        "#
    )]
    /// Exempt certain modules from needing to be moved into type-checking
    /// blocks.
    pub exempt_modules: Option<Vec<String>>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string()],
        }
    }
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            strict: options.strict.unwrap_or(false),
            exempt_modules: options
                .exempt_modules
                .unwrap_or_else(|| vec!["typing".to_string()]),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            strict: Some(settings.strict),
            exempt_modules: Some(settings.exempt_modules),
        }
    }
}
