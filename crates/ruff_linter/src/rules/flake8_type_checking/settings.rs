//! Settings for the `flake8-type-checking` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Serialize, Deserialize, CacheKey, Default,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum QuoteTypeExpressions {
    /// Never quote type expressions to keep imports in typing context.
    #[default]
    None,
    /// Only quote the type expressions that are safe to quote.
    Safe,
    /// Quote additional type expressions that should be safe as long
    /// runtime required symbols are properly configured.
    Balanced,
    /// Quote everything that could conceivably be quoted without leading
    /// to a detectable error in the same source file.
    Eager,
}

impl Display for QuoteTypeExpressions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "\"none\""),
            Self::Safe => write!(f, "\"safe\""),
            Self::Balanced => write!(f, "\"balanced\""),
            Self::Eager => write!(f, "\"eager\""),
        }
    }
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_required_base_classes: Vec<String>,
    pub runtime_required_decorators: Vec<String>,
    pub quote_type_expressions: QuoteTypeExpressions,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string(), "typing_extensions".to_string()],
            runtime_required_base_classes: vec![],
            runtime_required_decorators: vec![],
            quote_type_expressions: QuoteTypeExpressions::default(),
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
                self.quote_type_expressions,
            ]
        }
        Ok(())
    }
}
