use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;

use crate::define_simple_violation;
use ruff_macros::derive_message_formats;

define_simple_violation!(NonEmpty, "Docstring is empty");

/// D419
pub fn not_empty(checker: &mut Checker, docstring: &Docstring) -> bool {
    if !docstring.body.trim().is_empty() {
        return true;
    }

    if checker.settings.rules.enabled(&Rule::NonEmpty) {
        checker.diagnostics.push(Diagnostic::new(
            NonEmpty,
            Range::from_located(docstring.expr),
        ));
    }
    false
}
