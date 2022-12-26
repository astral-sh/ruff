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

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub ignore_variadic_names: bool,
}

impl Settings {
    #[allow(clippy::needless_pass_by_value)]
    pub fn from_options(options: Options) -> Self {
        Self {
            ignore_variadic_names: options.ignore_variadic_names.unwrap_or_default(),
        }
    }
}
