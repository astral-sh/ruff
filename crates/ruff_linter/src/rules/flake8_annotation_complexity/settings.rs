//! Settings for the `flake-annotation-complexity` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

pub const DEFAULT_MAX_ANNOTATION_COMPLEXITY: usize = 2;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub max_annotation_complexity: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_annotation_complexity: DEFAULT_MAX_ANNOTATION_COMPLEXITY,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_annotation_complexity",
            fields = [
                self.max_annotation_complexity,
            ]
        }
        Ok(())
    }
}
