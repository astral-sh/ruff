use ruff_python_ast::Identifier;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::rules::pycodestyle::helpers::is_ambiguous_name;

/// ## What it does
/// Checks for the use of the characters 'l', 'O', or 'I' as class names.
///
/// ## Why is this bad?
/// In some fonts, these characters are indistinguishable from the
/// numerals one and zero. When tempted to use 'l', use 'L' instead.
///
/// ## Example
///
/// ```python
/// class I(object): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class Integer(object): ...
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct AmbiguousClassName(pub String);

impl Violation for AmbiguousClassName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousClassName(name) = self;
        format!("Ambiguous class name: `{name}`")
    }
}

/// E742
pub(crate) fn ambiguous_class_name(name: &Identifier) -> Option<Diagnostic> {
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            AmbiguousClassName(name.to_string()),
            name.range(),
        ))
    } else {
        None
    }
}
