use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub functions_names: Vec<String>,
}

pub fn default_func_names() -> Vec<String> {
    vec![
        "_".to_string(),
        "gettext".to_string(),
        "ngettext".to_string(),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            functions_names: default_func_names(),
        }
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_gettext",
            fields = [
                self.functions_names | array
            ]
        }
        Ok(())
    }
}
