use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct QuotedAnnotations;
);
impl AlwaysAutofixableViolation for QuotedAnnotations {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Removed quotes from the type annotations")
    }

    fn autofix_title(&self) -> String {
        "Removed the quotes".to_string()
    }
}

/// UP037
pub fn quoted_annotations(checker: &mut Checker, annotation: &str, range: Range) {
    let mut diagnostic = Diagnostic::new(QuotedAnnotations, range);
    if checker.patch(&Rule::QuotedAnnotations) {
        diagnostic.amend(Fix::replacement(
            annotation.to_string(),
            range.location,
            range.end_location,
        ));
    }
    checker.diagnostics.push(diagnostic);
}
