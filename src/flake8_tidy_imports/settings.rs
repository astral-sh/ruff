//! Settings for the `flake8-tidy-imports` plugin.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum Strictness {
    Parents,
    All,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Options {
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
