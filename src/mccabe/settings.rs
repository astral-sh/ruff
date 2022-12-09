//! Settings for the `mccabe` plugin.

use ruff_macros::ConfigurationOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = "The maximum McCabe complexity to allow before triggering `C901` errors.",
        default = "10",
        value_type = "usize",
        example = r#"
            # Flag errors (`C901`) whenever the complexity level exceeds 5.
            max-complexity = 5
        "#
    )]
    pub max_complexity: Option<usize>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub max_complexity: usize,
}

impl Settings {
    pub fn from_options(options: &Options) -> Self {
        Self {
            max_complexity: options.max_complexity.unwrap_or_default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self { max_complexity: 10 }
    }
}
