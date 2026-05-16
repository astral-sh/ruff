//! Settings for the `ruff` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

pub const DEFAULT_MAX_ANNOTATION_COMPLEXITY: usize = 2;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub parenthesize_tuple_in_subscript: bool,
    pub strictly_empty_init_modules: bool,
    pub max_annotation_complexity: usize,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.ruff",
            fields = [
                self.parenthesize_tuple_in_subscript,
                self.strictly_empty_init_modules,
                self.max_annotation_complexity,
            ]
        }
        Ok(())
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            parenthesize_tuple_in_subscript: Default::default(),
            strictly_empty_init_modules: Default::default(),
            max_annotation_complexity: DEFAULT_MAX_ANNOTATION_COMPLEXITY,
        }
    }
}
