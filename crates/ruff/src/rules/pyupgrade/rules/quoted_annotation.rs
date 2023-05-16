use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;

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
pub(crate) fn quoted_annotation(checker: &mut Checker, annotation: &str, range: TextRange) {
    let mut diagnostic = Diagnostic::new(QuotedAnnotation, range);
    if checker.patch(Rule::QuotedAnnotation) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            annotation.to_string(),
            range,
        )));
    }
    checker.diagnostics.push(diagnostic);
}
