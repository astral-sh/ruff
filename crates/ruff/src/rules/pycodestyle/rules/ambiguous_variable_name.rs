use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::pycodestyle::helpers::is_ambiguous_name;

/// ## What it does
/// Checks for the use of the characters 'l', 'O', or 'I' as variable names.
///
/// ## Why is this bad?
/// In some fonts, these characters are indistinguishable from the
/// numerals one and zero. When tempted to use 'l', use 'L' instead.
///
/// ## Example
/// ```python
/// l = 0
/// O = 123
/// I = 42
/// ```
///
/// Use instead:
/// ```python
/// L = 0
/// o = 123
/// i = 42
/// ```

#[violation]
pub struct AmbiguousVariableName(pub String);

impl Violation for AmbiguousVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousVariableName(name) = self;
        format!("Ambiguous variable name: `{name}`")
    }
}

/// E741
pub(crate) fn ambiguous_variable_name(name: &str, range: TextRange) -> Option<Diagnostic> {
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            AmbiguousVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}
