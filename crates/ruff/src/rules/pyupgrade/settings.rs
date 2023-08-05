//! Settings for the `pyupgrade` plugin.

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "PyUpgradeOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
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
    /// (`Union[str, int]` -> `str | int`) rewrites even if a file imports
    /// `from __future__ import annotations`.
    ///
    /// This setting is only applicable when the target Python version is below
    /// 3.9 and 3.10 respectively, and is most commonly used when working with
    /// libraries like Pydantic and FastAPI, which rely on the ability to parse
    /// type annotations at runtime. The use of `from __future__ import annotations`
    /// causes Python to treat the type annotations as strings, which typically
    /// allows for the use of language features that appear in later Python
    /// versions but are not yet supported by the current version (e.g., `str |
    /// int`). However, libraries that rely on runtime type annotations will
    /// break if the annotations are incompatible with the current Python
    /// version.
    ///
    /// For example, while the following is valid Python 3.8 code due to the
    /// presence of `from __future__ import annotations`, the use of `str| int`
    /// prior to Python 3.10 will cause Pydantic to raise a `TypeError` at
    /// runtime:
    ///
    /// ```python
    /// from __future__ import annotations
    ///
    /// import pydantic
    ///
    /// class Foo(pydantic.BaseModel):
    ///    bar: str | int
    /// ```
    ///
    ///
    pub keep_runtime_typing: Option<bool>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub keep_runtime_typing: bool,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            keep_runtime_typing: options.keep_runtime_typing.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            keep_runtime_typing: Some(settings.keep_runtime_typing),
        }
    }
}
