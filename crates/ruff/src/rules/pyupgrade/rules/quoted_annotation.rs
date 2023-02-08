use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct QuotedAnnotation;
);
impl AlwaysAutofixableViolation for QuotedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Remove quotes from type annotation")
    }

    fn autofix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// UP037
pub fn quoted_annotation(checker: &mut Checker, annotation: &str, range: Range) {
    let mut diagnostic = Diagnostic::new(QuotedAnnotation, range);
    if checker.patch(&Rule::QuotedAnnotation) {
        diagnostic.amend(Fix::replacement(
            annotation.to_string(),
            range.location,
            range.end_location,
        ));
    }
    checker.diagnostics.push(diagnostic);
}
