use ruff_macros::CacheKey;

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
