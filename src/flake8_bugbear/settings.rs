//! Settings for the `flake8-bugbear` plugin.

use ruff_macros::ConfigurationOptions;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
    #[option(
        doc = r#"
            Additional callable functions to consider "immutable" when evaluating, e.g.,
            `no-mutable-default-argument` checks (`B006`).
        "#,
        default = r#"[]"#,
        value_type = "Vec<String>",
        example = r#"
            # Allow default arguments like, e.g., `data: List[str] = fastapi.Query(None)`.
            extend-immutable-calls = ["fastapi.Depends", "fastapi.Query"]
        "#
    )]
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
