use ruff_text_size::TextRange;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct QuotedAnnotationInStub;

impl AlwaysAutofixableViolation for QuotedAnnotationInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Quoted annotations should not be included in stubs")
    }

    fn autofix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// PYI020
pub(crate) fn quoted_annotation_in_stub(checker: &mut Checker, annotation: &str, range: TextRange) {
    let mut diagnostic = Diagnostic::new(QuotedAnnotationInStub, range);
    if checker.patch(Rule::QuotedAnnotationInStub) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            annotation.to_string(),
            range,
        )));
    }
    checker.diagnostics.push(diagnostic);
}
