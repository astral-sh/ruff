//! Settings for the `flake8-type-checking` plugin.
use std::error::Error;
use std::fmt::{Display, Formatter, Result};

use ruff_macros::CacheKey;

use crate::display_settings;
use crate::settings::types::IdentifierPattern;

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_required_base_classes: Vec<String>,
    pub runtime_required_decorators: Vec<IdentifierPattern>,
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

/// Error returned by the [`TryFrom`] implementation of [`Settings`].
#[derive(Debug)]
pub enum SettingsError {
    InvalidRuntimeEvaluatedDecorators(glob::PatternError),
}

impl Display for SettingsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            SettingsError::InvalidRuntimeEvaluatedDecorators(err) => {
                write!(f, "invalid runtime-evaluated-decorators pattern: {err}")
            }
        }
    }
}

impl Error for SettingsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SettingsError::InvalidRuntimeEvaluatedDecorators(err) => Some(err),
        }
    }
}
