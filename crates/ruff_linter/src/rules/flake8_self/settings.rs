//! Settings for the `flake8-self` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use ruff_python_ast::name::Name;
use std::fmt::{Display, Formatter};

// By default, ignore the `namedtuple` methods and attributes, as well as the
// _sunder_ names in Enum, which are underscore-prefixed to prevent conflicts
// with field names.
pub const IGNORE_NAMES: [&str; 7] = [
    "_make",
    "_asdict",
    "_replace",
    "_fields",
    "_field_defaults",
    "_name_",
    "_value_",
];

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub ignore_names: Vec<Name>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES.map(Name::new_static).to_vec(),
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_self",
            fields = [
                self.ignore_names | array
            ]
        }
        Ok(())
    }
}
