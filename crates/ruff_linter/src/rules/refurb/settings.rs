//! Settings for the `refurb` plugin.

use std::fmt;

use ruff_macros::CacheKey;
use rustc_hash::FxHashSet;

use crate::display_settings;

pub fn default_allow_abc_meta_bases() -> FxHashSet<String> {
    ["typing.Protocol", "typing_extensions.Protocol"]
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub allow_abc_meta_bases: FxHashSet<String>,
    pub extend_abc_meta_bases: FxHashSet<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_abc_meta_bases: default_allow_abc_meta_bases(),
            extend_abc_meta_bases: Default::default(),
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.refurb",
            fields = [
                self.allow_abc_meta_bases | set
            ]
        }
        Ok(())
    }
}
