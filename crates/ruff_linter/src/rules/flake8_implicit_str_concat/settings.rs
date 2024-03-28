//! Settings for the `flake8-implicit-str-concat` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub allow_multiline: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            allow_multiline: true,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_implicit_str_concat",
            fields = [
                self.allow_multiline
            ]
        }
        Ok(())
    }
}
