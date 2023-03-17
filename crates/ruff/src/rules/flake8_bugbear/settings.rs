//! Settings for the `flake8-bugbear` plugin.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8BugbearOptions"
)]
pub struct Options {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = r#"
            # Allow default arguments like, e.g., `data: List[str] = fastapi.Query(None)`.
            extend-immutable-calls = ["fastapi.Depends", "fastapi.Query"]
        "#
    )]
    /// Additional callable functions to consider "immutable" when evaluating,
    /// e.g., the `no-mutable-default-argument` rule (`B006`).
    pub extend_immutable_calls: Option<Vec<String>>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub extend_immutable_calls: Vec<String>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            extend_immutable_calls: options.extend_immutable_calls.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            extend_immutable_calls: Some(settings.extend_immutable_calls),
        }
    }
}
