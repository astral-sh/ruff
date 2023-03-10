use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::rules::pycodestyle::helpers::is_ambiguous_name;

#[violation]
pub struct AmbiguousClassName(pub String);

impl Violation for AmbiguousClassName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousClassName(name) = self;
        format!("Ambiguous class name: `{name}`")
    }
}

/// E742
pub fn ambiguous_class_name<F>(name: &str, locate: F) -> Option<Diagnostic>
where
    F: FnOnce() -> Range,
{
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            AmbiguousClassName(name.to_string()),
            locate(),
        ))
    } else {
        None
    }
}
