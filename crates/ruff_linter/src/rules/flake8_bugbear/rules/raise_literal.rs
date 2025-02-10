use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `raise` statements that raise a literal value.
///
/// ## Why is this bad?
/// `raise` must be followed by an exception instance or an exception class,
/// and exceptions must be instances of `BaseException` or a subclass thereof.
/// Raising a literal will raise a `TypeError` at runtime.
///
/// ## Example
/// ```python
/// raise "foo"
/// ```
///
/// Use instead:
/// ```python
/// raise Exception("foo")
/// ```
///
/// ## References
/// - [Python documentation: `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
#[derive(ViolationMetadata)]
pub(crate) struct RaiseLiteral;

impl Violation for RaiseLiteral {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Cannot raise a literal. Did you intend to return it or raise an Exception?".to_string()
    }
}

/// B016
pub(crate) fn raise_literal(checker: &Checker, expr: &Expr) {
    if expr.is_literal_expr() {
        checker.report_diagnostic(Diagnostic::new(RaiseLiteral, expr.range()));
    }
}
