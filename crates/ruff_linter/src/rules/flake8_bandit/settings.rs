//! Settings for the `flake8-bandit` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

pub fn default_tmp_dirs() -> Vec<String> {
    ["/tmp", "/var/tmp", "/dev/shm"]
        .map(ToString::to_string)
        .to_vec()
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub hardcoded_tmp_directory: Vec<String>,
    pub check_typed_exception: bool,
    pub extend_markup_names: Vec<String>,
    pub allowed_markup_calls: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hardcoded_tmp_directory: default_tmp_dirs(),
            check_typed_exception: false,
            extend_markup_names: vec![],
            allowed_markup_calls: vec![],
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_bandit",
            fields = [
                self.hardcoded_tmp_directory | array,
                self.check_typed_exception,
                self.extend_markup_names | array,
                self.allowed_markup_calls | array,
            ]
        }
        Ok(())
    }
}
