//! Settings for the `flake8-copyright` plugin.

use once_cell::sync::Lazy;
use regex::Regex;

use ruff_macros::CacheKey;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub notice_rgx: Regex,
    pub author: Option<String>,
    pub min_file_size: usize,
}

pub static COPYRIGHT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)Copyright\s+(\(C\)\s+)?\d{4}(-\d{4})*").unwrap());

impl Default for Settings {
    fn default() -> Self {
        Self {
            notice_rgx: COPYRIGHT.clone(),
            author: None,
            min_file_size: 0,
        }
    }
}
