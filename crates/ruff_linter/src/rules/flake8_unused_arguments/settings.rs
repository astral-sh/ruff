//! Settings for the `flake8-unused-arguments` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub ignore_variadic_names: bool,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_unused_arguments",
            fields = [
                self.ignore_variadic_names
            ]
        }
        Ok(())
    }
}
