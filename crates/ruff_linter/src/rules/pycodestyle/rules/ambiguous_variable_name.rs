use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};

use crate::checkers::ast::Checker;
use crate::rules::pycodestyle::helpers::is_ambiguous_name;

/// ## What it does
/// Checks for the use of the characters 'l', 'O', or 'I' as variable names.
///
/// Note: This rule is automatically disabled for all stub files
/// (files with `.pyi` extensions). The rule has little relevance for authors
/// of stubs: a well-written stub should aim to faithfully represent the
/// interface of the equivalent .py file as it exists at runtime, including any
/// ambiguously named variables in the runtime module.
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
///
#[derive(ViolationMetadata)]
pub(crate) struct AmbiguousVariableName(pub String);

impl Violation for AmbiguousVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousVariableName(name) = self;
        format!("Ambiguous variable name: `{name}`")
    }
}

/// E741
pub(crate) fn ambiguous_variable_name(checker: &Checker, name: &str, range: TextRange) {
    if checker.source_type.is_stub() {
        return;
    }
    if is_ambiguous_name(name) {
        checker.report_diagnostic(Diagnostic::new(
            AmbiguousVariableName(name.to_string()),
            range,
        ));
    }
}
