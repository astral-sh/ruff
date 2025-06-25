use ruff_text_size::TextRange;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for quoted type annotations in stub (`.pyi`) files, which should be avoided.
///
/// ## Why is this bad?
/// Stub files natively support forward references in all contexts, as stubs
/// are never executed at runtime. (They should be thought of as "data files"
/// for type checkers and IDEs.) As such, quotes are never required for type
/// annotations in stub files, and should be omitted.
///
/// ## Example
///
/// ```pyi
/// def function() -> "int": ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def function() -> int: ...
/// ```
///
/// ## References
/// - [Typing documentation - Writing and Maintaining Stub Files](https://typing.python.org/en/latest/guides/writing_stubs.html)
#[derive(ViolationMetadata)]
pub(crate) struct QuotedAnnotationInStub;

impl AlwaysFixableViolation for QuotedAnnotationInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Quoted annotations should not be included in stubs".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// PYI020
pub(crate) fn quoted_annotation_in_stub(checker: &Checker, annotation: &str, range: TextRange) {
    let mut diagnostic = checker.report_diagnostic(QuotedAnnotationInStub, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        annotation.to_string(),
        range,
    )));
}
