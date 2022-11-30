//! Settings for the `pyupgrade` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub keep_runtime_typing: Option<bool>,
}

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub keep_runtime_typing: bool,
}

impl Settings {
    pub fn from_options(options: &Options) -> Self {
        Self {
            keep_runtime_typing: options.keep_runtime_typing.unwrap_or_default(),
        }
    }
}
