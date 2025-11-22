//! Settings for the `flake8-commas` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub allow_single_arg_function_calls: bool,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_commas",
            fields = [
                self.allow_single_arg_function_calls
            ]
        }
        Ok(())
    }
}
