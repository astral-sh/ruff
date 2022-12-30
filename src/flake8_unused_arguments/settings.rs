//! Settings for the `flake8-unused-arguments` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8UnusedArgumentsOptions"
)]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = "ignore-variadic-names = true"
    )]
    /// Whether to allow unused variadic arguments, like `*args` and `**kwargs`.
    pub ignore_variadic_names: Option<bool>,
}

#[derive(Debug, Default, Hash)]
pub struct Settings {
    pub ignore_variadic_names: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            ignore_variadic_names: options.ignore_variadic_names.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            ignore_variadic_names: Some(settings.ignore_variadic_names),
        }
    }
}
