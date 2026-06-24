//! Settings for the `ruff` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub parenthesize_tuple_in_subscript: bool,
    pub strictly_empty_init_modules: bool,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.ruff",
            fields = [
                self.parenthesize_tuple_in_subscript,
                self.strictly_empty_init_modules,
            ]
        }
        Ok(())
    }
}
