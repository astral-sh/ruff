//! Settings for the `flake8-django` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub additional_path_functions: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            additional_path_functions: vec![],
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_django",
            fields = [
                self.additional_path_functions | array
            ]
        }
        Ok(())
    }
}
