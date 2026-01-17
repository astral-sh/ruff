//! Settings for the `flake-annotation-complexity` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub max_annotation_complexity: isize,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_annotation_complexity",
            fields = [
                self.max_annotation_complexity,
            ]
        }
        Ok(())
    }
}
