//! Settings for the `Pyflakes` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub extend_generics: Vec<String>,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pyflakes",
            fields = [
                self.extend_generics | debug
            ]
        }
        Ok(())
    }
}
