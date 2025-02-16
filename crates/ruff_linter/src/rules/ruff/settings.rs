//! Settings for the `ruff` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ArgsMadeMandatory {
    /// The args to make mandatory when the API is used.
    pub args: Vec<String>,
}

impl fmt::Display for ArgsMadeMandatory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.args)
    }
}

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub parenthesize_tuple_in_subscript: bool,
    pub extend_markup_names: Vec<String>,
    pub allowed_markup_calls: Vec<String>,
    pub optional_made_mandatory: FxHashMap<String, ArgsMadeMandatory>,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.ruff",
            fields = [
                self.parenthesize_tuple_in_subscript,
                self.extend_markup_names | array,
                self.allowed_markup_calls | array,
                self.optional_made_mandatory | map,
            ]
        }
        Ok(())
    }
}
