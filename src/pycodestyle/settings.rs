//! Settings for the `pycodestyle` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", rename = "Pycodestyle")]
pub struct Options {}

#[derive(Debug, Default, Hash)]
pub struct Settings {}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {}
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {}
    }
}
