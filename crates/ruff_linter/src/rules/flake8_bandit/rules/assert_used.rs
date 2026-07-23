use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;
use ruff_text_size::{TextLen, TextRange};

use crate::Violation;

use crate::checkers::ast::Checker;

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
/// ## Type narrowing
/// `assert` is sometimes used to narrow types for static type checkers, e.g.
/// `assert x is not None`. Since these assertions are only useful to the
/// type checker and have no runtime effect worth preserving, they are often
/// better placed under a `TYPE_CHECKING` guard so they are not executed at
/// runtime:
///
/// ```python
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     assert isinstance(x, int)
/// ```
///
/// Alternatively, if the assertion is needed at runtime, suppress the rule
/// at the call site with `# noqa: S101`.
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
#[violation_metadata(stable_since = "v0.0.116")]
pub(crate) struct Assert;

impl Violation for Assert {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `assert` detected".to_string()
    }
}

/// S101
pub(crate) fn assert_used(checker: &Checker, stmt: &Stmt) {
    checker.report_diagnostic(Assert, TextRange::at(stmt.start(), "assert".text_len()));
}
