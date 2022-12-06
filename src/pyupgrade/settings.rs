//! Settings for the `pyupgrade` plugin.

use ruff_macros::ConfigurationOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = r#"
            Whether to avoid PEP 585 (`List[int]` -> `list[int]`) and PEP 604 (`Optional[str]` -> `str | None`) rewrites even if a file imports `from __future__ import annotations`. Note that this setting is only applicable when the target Python version is below 3.9 and 3.10 respectively.
        "#,
        default = r#"false"#,
        value_type = "bool",
        example = r#"
            # Preserve types, even if a file imports `from __future__ import annotations`.
            keep-runtime-typing = true
        "#
    )]
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
