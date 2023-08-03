use ruff_text_size::TextRange;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for the presence of unnecessary quotes in type annotations.
///
/// ## Why is this bad?
/// In Python, type annotations can be quoted to avoid forward references.
/// However, if `from __future__ import annotations` is present, Python
/// will always evaluate type annotations in a deferred manner, making
/// the quotes unnecessary.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
///
/// def foo(bar: "Bar") -> "Bar":
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
///
/// def foo(bar: Bar) -> Bar:
///     ...
/// ```
///
/// ## References
/// - [PEP 563](https://peps.python.org/pep-0563/)
/// - [Python documentation: `__future__`](https://docs.python.org/3/library/__future__.html#module-__future__)
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
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            annotation.to_string(),
            range,
        )));
    }
    checker.diagnostics.push(diagnostic);
}
