//! Settings for the `flake8-bugbear` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub extend_immutable_calls: Vec<String>,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_bugbear",
            fields = [
                self.extend_immutable_calls | array
            ]
        }
        Ok(())
    }
}
