use ruff_text_size::TextRange;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for quoted type annotations in stub (`.pyi`) files, which should be avoided.
///
/// ## Why is this bad?
/// Stub files are evaluated using `annotations` semantics, as if
/// `from __future__ import annotations` were included in the file. As such,
/// quotes are never required for type annotations in stub files, and should be
/// omitted.
///
/// ## Example
/// ```python
/// def function() -> "int":
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def function() -> int:
///     ...
/// ```
#[violation]
pub struct QuotedAnnotationInStub;

impl AlwaysFixableViolation for QuotedAnnotationInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Quoted annotations should not be included in stubs")
    }

    fn fix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// PYI020
pub(crate) fn quoted_annotation_in_stub(checker: &mut Checker, annotation: &str, range: TextRange) {
    let mut diagnostic = Diagnostic::new(QuotedAnnotationInStub, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        annotation.to_string(),
        range,
    )));
    checker.diagnostics.push(diagnostic);
}
