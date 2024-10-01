//! Settings for the `flake-annotations` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Default, CacheKey)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub mypy_init_return: bool,
    pub suppress_dummy_args: bool,
    pub suppress_none_returning: bool,
    pub allow_star_arg_any: bool,
    pub ignore_fully_untyped: bool,
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_annotations",
            fields = [
                self.mypy_init_return,
                self.suppress_dummy_args,
                self.suppress_none_returning,
                self.allow_star_arg_any,
                self.ignore_fully_untyped
            ]
        }
        Ok(())
    }
}
