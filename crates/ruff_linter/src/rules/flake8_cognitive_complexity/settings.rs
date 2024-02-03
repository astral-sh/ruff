//! Settings for the `flake8_cognitive_complexity` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub max_cognitive_complexity: usize,
}

pub const DEFAULT_MAX_COGNITIVE_COMPLEXITY: usize = 7;

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_cognitive_complexity: DEFAULT_MAX_COGNITIVE_COMPLEXITY,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_cognitive_complexity",
            fields = [
                self.max_cognitive_complexity
            ]
        }
        Ok(())
    }
}
