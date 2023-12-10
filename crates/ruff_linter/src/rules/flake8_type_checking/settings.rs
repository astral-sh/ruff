//! Settings for the `flake8-type-checking` plugin.

use ruff_macros::CacheKey;
use serde::{Deserialize, Serialize};

#[derive(Debug, CacheKey)]
pub struct Settings {
    pub strict: bool,
    pub exempt_modules: Vec<String>,
    pub runtime_evaluated_base_classes: Vec<String>,
    pub runtime_evaluated_decorators: Vec<String>,
    pub annotation_strategy: AnnotationStrategy,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict: false,
            exempt_modules: vec!["typing".to_string(), "typing_extensions".to_string()],
            runtime_evaluated_base_classes: vec![],
            runtime_evaluated_decorators: vec![],
            annotation_strategy: AnnotationStrategy::Preserve,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, CacheKey)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum AnnotationStrategy {
    /// Avoid changing the semantics of runtime-evaluated annotations (e.g., by quoting them, or
    /// inserting `from __future__ import annotations`). Imports will be classified as typing-only
    /// or runtime-required based exclusively on the existing type annotations.
    #[default]
    Preserve,
    /// Quote runtime-evaluated annotations, if doing so would enable the corresponding import to
    /// be moved into an `if TYPE_CHECKING:` block.
    Quote,
    /// Insert `from __future__ import annotations` at the top of the file, if doing so would enable
    /// an import to be moved into an `if TYPE_CHECKING:` block.
    Future,
}
