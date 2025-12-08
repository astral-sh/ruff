//! Settings for the `flake8-lineleak` plugin.

use std::fmt::{Display, Formatter};

use ruff_macros::CacheKey;

use crate::display_settings;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub max_line_count: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_line_count: 100,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_lineleak",
            fields = [
                self.max_line_count,
            ]
        }
        Ok(())
    }
}
