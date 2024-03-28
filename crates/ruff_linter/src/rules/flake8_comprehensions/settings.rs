//! Settings for the `flake8-comprehensions` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub allow_dict_calls_with_keyword_arguments: bool,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_comprehensions",
            fields = [
                self.allow_dict_calls_with_keyword_arguments
            ]
        }
        Ok(())
    }
}
