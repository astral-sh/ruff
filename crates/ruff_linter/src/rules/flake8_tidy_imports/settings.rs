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
pub enum ImportStyle {
    /// Force imports to be relative.
    AlwaysRelative,
    /// Ban imports that extend into the parent module or beyond.
    #[default]
    ParentsAbsolute,
    /// Ban all relative imports.
    AlwaysAbsolute,
}

impl Display for ImportStyle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlwaysRelative => write!(f, "\"always-relative\""),
            Self::ParentsAbsolute => write!(f, "\"parents-absolute\""),
            Self::AlwaysAbsolute => write!(f, "\"always-absolute\""),
        }
    }
}

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub relative_import_style: ImportStyle,
    pub banned_api: FxHashMap<String, ApiBan>,
    pub banned_module_level_imports: Vec<String>,
}

impl Settings {
    pub fn banned_module_level_imports(&self) -> impl Iterator<Item = &str> {
        self.banned_module_level_imports.iter().map(AsRef::as_ref)
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_tidy_imports",
            fields = [
                self.relative_import_style,
                self.banned_api | map,
                self.banned_module_level_imports | array,
            ]
        }
        Ok(())
    }
}
