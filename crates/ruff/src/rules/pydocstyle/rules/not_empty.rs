use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;

define_violation!(
    pub struct NonEmpty;
);
impl Violation for NonEmpty {
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

    if checker.settings.rules.enabled(&Rule::NonEmpty) {
        checker.diagnostics.push(Diagnostic::new(
            NonEmpty,
            Range::from_located(docstring.expr),
        ));
    }
    false
}
