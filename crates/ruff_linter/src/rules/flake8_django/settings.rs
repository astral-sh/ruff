//! Settings for the `flake8-django` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub additional_path_functions: Vec<String>,
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
