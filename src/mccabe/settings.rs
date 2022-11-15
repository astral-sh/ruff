//! Settings for the `mccabe` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    pub max_complexity: Option<isize>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub max_complexity: isize,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            max_complexity: options.max_complexity.unwrap_or_default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self { max_complexity: -1 }
    }
}
