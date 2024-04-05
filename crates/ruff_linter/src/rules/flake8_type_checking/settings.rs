//! Settings for the `flake8-type-checking` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, CacheKey)]
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

impl Display for Settings {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_settings! {
            formatter = f,
            namespace = "linter.flake8_type_checking",
            fields = [
                self.strict,
                self.exempt_modules | array,
                self.runtime_required_base_classes | array,
                self.runtime_required_decorators | array,
                self.quote_annotations
            ]
        }
        Ok(())
    }
}
