use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::display_settings;
use ruff_macros::CacheKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ApiBan {
    /// The message to display when the API is used.
    pub msg: String,
}

impl Display for ApiBan {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Strictness {
    /// Ban imports that extend into the parent module or beyond.
    #[default]
    Parents,
    /// Ban all relative imports.
    All,
}

impl Display for Strictness {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parents => write!(f, "\"parents\""),
            Self::All => write!(f, "\"all\""),
        }
    }
}

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub ban_relative_imports: Strictness,
    pub banned_api: FxHashMap<String, ApiBan>,
    pub banned_module_level_imports: Vec<String>,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_tidy_imports",
            fields = [
                self.ban_relative_imports,
                self.banned_api | map,
                self.banned_module_level_imports | array,
            ]
        }
        Ok(())
    }
}
