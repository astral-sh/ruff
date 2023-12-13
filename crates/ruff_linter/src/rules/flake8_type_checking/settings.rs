//! Settings for the `flake8-type-checking` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_required_base_classes: Vec<String>,
    pub runtime_required_decorators: Vec<String>,
    pub quote_annotations: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string(), "typing_extensions".to_string()],
            runtime_required_base_classes: vec![],
            runtime_required_decorators: vec![],
            quote_annotations: false,
        }
    }
}
