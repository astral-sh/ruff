//! Settings for the `pyupgrade` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt;

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub keep_runtime_typing: bool,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pyupgrade",
            fields = [
                self.keep_runtime_typing
            ]
        }
        Ok(())
    }
}
