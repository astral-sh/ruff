//! Settings for the `pep8-naming` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub extend_immutable_calls: Option<Vec<String>>,
}

#[derive(Debug, Hash, Default)]
pub struct Settings {
    pub extend_immutable_calls: Vec<String>,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            extend_immutable_calls: options.extend_immutable_calls.unwrap_or_default(),
        }
    }
}
