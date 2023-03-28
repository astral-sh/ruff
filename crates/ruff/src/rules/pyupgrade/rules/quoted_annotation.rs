use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct QuotedAnnotation;

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
    if checker.patch(Rule::QuotedAnnotation) {
        diagnostic.set_fix(Edit::replacement(
            annotation.to_string(),
            range.location,
            range.end_location,
        ));
    }
    checker.diagnostics.push(diagnostic);
}
