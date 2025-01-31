//! Settings for the `pydoclint` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
pub struct Settings {
    pub ignore_one_line_docstrings: bool,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.pydoclint",
            fields = [self.ignore_one_line_docstrings]
        }
        Ok(())
    }
}
