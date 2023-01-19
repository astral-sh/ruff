use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// D419
pub fn not_empty(checker: &mut Checker, docstring: &Docstring) -> bool {
    if !docstring.body.trim().is_empty() {
        return true;
    }

    if checker.settings.rules.enabled(&Rule::NonEmpty) {
        checker.diagnostics.push(Diagnostic::new(
            violations::NonEmpty,
            Range::from_located(docstring.expr),
        ));
    }
    false
}
