//! Settings for the `flake8-copyright` plugin.

use std::fmt::{Display, Formatter};
use std::sync::LazyLock;

use regex::Regex;

use ruff_macros::CacheKey;

use crate::display_settings;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub notice_rgx: Regex,
    pub author: Option<String>,
    pub min_file_size: usize,
}

pub static COPYRIGHT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Copyright\s+((?:\(C\)|©)\s+)?\d{4}((-|,\s)\d{4})*").unwrap());

impl Default for Settings {
    fn default() -> Self {
        Self {
            notice_rgx: COPYRIGHT.clone(),
            author: None,
            min_file_size: 0,
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_copyright",
            fields = [
                self.notice_rgx,
                self.author | optional,
                self.min_file_size,
            ]
        }
        Ok(())
    }
}
