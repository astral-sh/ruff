use ruff_python_ast::Stmt;
use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of the `assert` keyword.
///
/// ## Why is this bad?
/// Assertions are removed when Python is run with optimization requested
/// (i.e., when the `-O` flag is present), which is a common practice in
/// production environments. As such, assertions should not be used for runtime
/// validation of user input or to enforce  interface constraints.
///
/// Consider raising a meaningful error instead of using `assert`.
///
/// ## Example
/// ```python
/// assert x > 0, "Expected positive value."
/// ```
///
/// Use instead:
/// ```python
/// if not x > 0:
///     raise ValueError("Expected positive value.")
///
/// # or even better:
/// if x <= 0:
///     raise ValueError("Expected positive value.")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct Assert;

impl Violation for Assert {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `assert` detected".to_string()
    }
}

/// S101
pub(crate) fn assert_used(stmt: &Stmt) -> Diagnostic {
    Diagnostic::new(Assert, TextRange::at(stmt.start(), "assert".text_len()))
}
