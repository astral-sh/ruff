//! Settings for the `refurb` plugin.

use std::fmt;

use ruff_macros::CacheKey;
use rustc_hash::FxHashSet;

use crate::display_settings;

pub fn default_allowed_abc_meta_bases() -> FxHashSet<String> {
    ["typing.Protocol", "typing_extensions.Protocol"]
        .into_iter()
        .map(ToString::to_string)
        .collect()
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub allowed_abc_meta_bases: FxHashSet<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allowed_abc_meta_bases: default_allowed_abc_meta_bases(),
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.refurb",
            fields = [
                self.allowed_abc_meta_bases | set
            ]
        }
        Ok(())
    }
}
