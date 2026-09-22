//! Settings for the `flake8-type-checking` plugin.

use crate::display_settings;
use ruff_macros::CacheKey;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// Represents the desired runtime semantics for a set of type definitions
///
/// Note that the order of variants is important here. `Required` has the
/// highest precedence when calling `RuntimeSemantics::combine` on two
/// separate targeting sources. (E.g. classes can be targeted both via
/// their decorators, but also via their base classes, so this determines
/// what happens when two sources disagree)
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    CacheKey,
    Default,
    is_macro::Is,
)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum RuntimeSemantics {
    /// Assume that all targeted annotations are going to be evaluated at runtime
    /// at some point. So all referenced bindings need to be available at runtime.
    #[default]
    Required,
    /// Do not make any assumptions about how the targeted annotations are going
    /// to be interacted with, assume the current execution context of any referenced
    /// bindings is already correct.
    Ambiguous,
    /// Use the default runtime semantics of the targeted annotations, this
    /// exists so that child configurations can revert entries the parent
    /// configuration configured back to the default semantics.
    Default,
}

impl RuntimeSemantics {
    pub(crate) fn combine(self, other: RuntimeSemantics) -> RuntimeSemantics {
        self.min(other)
    }
}

impl Display for RuntimeSemantics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required => write!(f, "\"required\""),
            Self::Ambiguous => write!(f, "\"ambiguous\""),
            Self::Default => write!(f, "\"default\""),
        }
    }
}

#[derive(Debug, Clone, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_evaluated_base_classes: FxHashMap<String, RuntimeSemantics>,
    pub runtime_evaluated_decorators: FxHashMap<String, RuntimeSemantics>,
    pub quote_annotations: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string(), "typing_extensions".to_string()],
            runtime_evaluated_base_classes: FxHashMap::default(),
            runtime_evaluated_decorators: FxHashMap::default(),
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
                self.runtime_evaluated_base_classes | map,
                self.runtime_evaluated_decorators | map,
                self.quote_annotations
            ]
        }
        Ok(())
    }
}
