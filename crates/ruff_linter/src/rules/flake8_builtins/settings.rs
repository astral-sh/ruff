//! Settings for the `flake8-builtins` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub ignorelist: Vec<String>,
    pub allowed_modules: Vec<String>,
    pub strict_checking: bool,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_builtins",
            fields = [
                self.allowed_modules | array,
                self.ignorelist | array,
                self.strict_checking,
            ]
        }
        Ok(())
    }
}
