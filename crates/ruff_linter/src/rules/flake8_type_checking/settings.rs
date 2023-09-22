//! Settings for the `flake8-type-checking` plugin.

use ruff_macros::CacheKey;

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_evaluated_base_classes: Vec<String>,
    pub runtime_evaluated_decorators: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string()],
            runtime_evaluated_base_classes: vec![],
            runtime_evaluated_decorators: vec![],
        }
    }
}
