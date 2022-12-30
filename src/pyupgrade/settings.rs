//! Settings for the `pyupgrade` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "PyUpgradeOptions"
)]
pub struct Options {
    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            # Preserve types, even if a file imports `from __future__ import annotations`.
            keep-runtime-typing = true
        "#
    )]
    /// Whether to avoid PEP 585 (`List[int]` -> `list[int]`) and PEP 604
    /// (`Optional[str]` -> `str | None`) rewrites even if a file imports `from
    /// __future__ import annotations`. Note that this setting is only
    /// applicable when the target Python version is below 3.9 and 3.10
    /// respectively.
    pub keep_runtime_typing: Option<bool>,

    #[option(
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            # Does not replace `mock` imports with `from unittest import mock`.
            keep-mock = true
        "#
    )]
    /// Whether to avoid PEP 585 (`List[int]` -> `list[int]`) and PEP 604
    /// (`Optional[str]` -> `str | None`) rewrites even if a file imports `from
    /// __future__ import annotations`. Note that this setting is only
    /// applicable when the target Python version is below 3.9 and 3.10
    /// respectively.
    pub keep_mock: Option<bool>,
}

#[derive(Debug, Default, Hash)]
pub struct Settings {
    pub keep_runtime_typing: bool,
    pub keep_mock: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            keep_runtime_typing: options.keep_runtime_typing.unwrap_or_default(),
            keep_mock: options.keep_mock.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            keep_runtime_typing: Some(settings.keep_runtime_typing),
            keep_mock: Some(settings.keep_mock),
        }
    }
}
