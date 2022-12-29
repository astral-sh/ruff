//! Settings for the `flake8-tidy-imports` plugin.

use ruff_macros::ConfigurationOptions;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    Parents,
    /// Ban all relative imports.
    All,
}

#[derive(
    Debug, PartialEq, Eq, Serialize, Deserialize, Default, ConfigurationOptions, JsonSchema,
)]
#[serde(
    deny_unknown_fields,
    rename_all = "kebab-case",
    rename = "Flake8TidyImportsOptions"
)]
pub struct Options {
    #[option(
        default = r#""parents""#,
        value_type = "Strictness",
        example = r#"
            # Disallow all relative imports.
            ban-relative-imports = "all"
        "#
    )]
    /// Whether to ban all relative imports (`"all"`), or only those imports
    /// that extend into the parent module or beyond (`"parents"`).
    pub ban_relative_imports: Option<Strictness>,
}

#[derive(Debug, Hash)]
pub struct Settings {
    pub ban_relative_imports: Strictness,
}

impl Settings {
    pub fn from_options(options: Options) -> Self {
        Self {
            ban_relative_imports: options.ban_relative_imports.unwrap_or(Strictness::Parents),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ban_relative_imports: Strictness::Parents,
        }
    }
}
