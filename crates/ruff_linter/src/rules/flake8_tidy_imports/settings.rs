use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use ruff_macros::CacheKey;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ApiBan {
    /// The message to display when the API is used.
    pub msg: String,
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

#[derive(Debug, CacheKey, Default)]
pub struct Settings {
    pub ban_relative_imports: Strictness,
    pub banned_api: FxHashMap<String, ApiBan>,
    pub banned_module_level_imports: Vec<String>,
}
