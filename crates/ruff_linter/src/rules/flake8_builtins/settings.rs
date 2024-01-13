//! Settings for the `flake8-builtins` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Default, CacheKey)]
pub struct Settings {
    pub builtins_ignorelist: Vec<String>,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_builtins",
            fields = [
                self.builtins_ignorelist | array
            ]
        }
        Ok(())
    }
}
