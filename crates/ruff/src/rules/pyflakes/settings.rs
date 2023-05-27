//! Settings for the `flake8-bugbear` plugin.

use serde::{Deserialize, Serialize};

use ruff_macros::{CacheKey, CombineOptions, ConfigurationOptions};

#[derive(
    Debug, PartialEq, Eq, Default, Serialize, Deserialize, ConfigurationOptions, CombineOptions,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "PyflakesOptions"
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Options {
    #[option(
        default = r#"[]"#,
        value_type = "list[str]",
        example = "extend-annotated-subscripts = [\"django.db.models.ForeignKey\"]"
    )]
    /// Additional subcripts to consider as a generic, e.g., `ForeignKey` in
    /// `django.db.models.ForeignKey["User"]`.
    pub extend_annotated_subscripts: Option<Vec<String>>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub extend_annotated_subscripts: Vec<String>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            extend_annotated_subscripts: options.extend_annotated_subscripts.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            extend_annotated_subscripts: Some(settings.extend_annotated_subscripts),
        }
    }
}
