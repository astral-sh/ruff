//! Settings for the `Pyflakes` plugin.

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
        example = "extend-generics = [\"django.db.models.ForeignKey\"]"
    )]
    /// Additional functions or classes to consider generic, such that any
    /// subscripts should be treated as type annotation (e.g., `ForeignKey` in
    /// `django.db.models.ForeignKey["User"]`.
    pub extend_generics: Option<Vec<String>>,
}

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub extend_generics: Vec<String>,
}

impl From<Options> for Settings {
    fn from(options: Options) -> Self {
        Self {
            extend_generics: options.extend_generics.unwrap_or_default(),
        }
    }
}

impl From<Settings> for Options {
    fn from(settings: Settings) -> Self {
        Self {
            extend_generics: Some(settings.extend_generics),
        }
    }
}
