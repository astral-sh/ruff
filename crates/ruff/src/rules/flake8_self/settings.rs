//! Settings for the `flake8-self` plugin.

use ruff_macros::CacheKey;

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

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub ignore_names: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ignore_names: IGNORE_NAMES.map(String::from).to_vec(),
        }
    }
}
