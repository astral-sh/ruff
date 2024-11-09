//! Settings for the `flake8-markupsafe` plugin.

use std::fmt;

use ruff_macros::CacheKey;

use crate::display_settings;

#[derive(Debug, Clone, CacheKey, Default)]
pub struct Settings {
    pub extend_markup_names: Vec<String>,
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_markupsafe",
            fields = [
                self.extend_markup_names | array,
            ]
        }
        Ok(())
    }
}
