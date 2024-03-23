//! Settings for the `flake8_boolean_trap` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub extend_allowed_calls: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            extend_allowed_calls: Vec::new(),
        }
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_boolean_trap",
            fields = [
                self.extend_allowed_calls | array,
            ]
        }
        Ok(())
    }
}
