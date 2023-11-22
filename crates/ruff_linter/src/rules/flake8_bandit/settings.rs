//! Settings for the `flake8-bandit` plugin.

use ruff_macros::CacheKey;

pub fn default_tmp_dirs() -> Vec<String> {
    ["/tmp", "/var/tmp", "/dev/shm"]
        .map(ToString::to_string)
        .to_vec()
}

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub hardcoded_tmp_directory: Vec<String>,
    pub check_typed_exception: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hardcoded_tmp_directory: default_tmp_dirs(),
            check_typed_exception: false,
        }
    }
}
