use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Rule;

#[violation]
pub struct EmptyDocstring;

impl Violation for EmptyDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Docstring is empty")
    }
}

/// D419
pub fn not_empty(checker: &mut Checker, docstring: &Docstring) -> bool {
    if !docstring.body.trim().is_empty() {
        return true;
    }

    if checker.settings.rules.enabled(Rule::EmptyDocstring) {
        checker
            .diagnostics
            .push(Diagnostic::new(EmptyDocstring, Range::from(docstring.expr)));
    }
    false
}
