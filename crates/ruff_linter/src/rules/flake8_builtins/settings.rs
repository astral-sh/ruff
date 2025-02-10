//! Settings for the `flake8-builtins` plugin.

use crate::{display_settings, settings::types::PreviewMode};
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub builtins_ignorelist: Vec<String>,
    pub builtins_allowed_modules: Vec<String>,
    pub builtins_strict_checking: bool,
}

impl Settings {
    pub fn new(preview: PreviewMode) -> Self {
        Self {
            builtins_ignorelist: Vec::new(),
            builtins_allowed_modules: Vec::new(),
            builtins_strict_checking: preview.is_disabled(),
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_builtins",
            fields = [
                self.builtins_allowed_modules | array,
                self.builtins_ignorelist | array,
                self.builtins_strict_checking,
            ]
        }
        Ok(())
    }
}
