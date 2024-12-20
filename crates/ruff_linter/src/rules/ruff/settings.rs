//! Settings for the `ruff` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub parenthesize_tuple_in_subscript: bool,
    pub extend_markup_names: Vec<String>,
    pub allowed_markup_calls: Vec<String>,
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
            ]
        }
        Ok(())
    }
}
