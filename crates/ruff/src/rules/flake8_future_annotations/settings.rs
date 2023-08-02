//! Settings for the `flake8-future-annotations` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8FutureAnnotationsOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = "false",
        value_type = "bool",
        example = "require-future-annotations = true"
    )]
    /// Require `from __future__ import annotations` in all modules
    /// where type annotations are used.
    pub require_future_annotations: Option<bool>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub require_future_annotations: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            require_future_annotations: options.require_future_annotations.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            require_future_annotations: Some(settings.require_future_annotations),
        }
    }
}
