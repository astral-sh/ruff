use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for assert tests that are non-empty tuples.
///
/// ## Why is this bad?
/// Assert tests are boolean expressions. Non-empty tuples are always `True`.
/// This means that the assert statement will always pass, which is likely a
/// mistake.
///
/// ## Example
/// ```python
/// assert (False,)  # always passes
/// ```
///
/// Use instead:
/// ```python
/// assert False  # always fails
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement)
#[violation]
pub struct AssertTuple;

impl Violation for AssertTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Assert test is a non-empty tuple, which is always `True`")
    }
}

/// F631
pub(crate) fn assert_tuple(checker: &mut Checker, stmt: &Stmt, test: &Expr) {
    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = &test {
        if !elts.is_empty() {
            checker
                .diagnostics
                .push(Diagnostic::new(AssertTuple, stmt.range()));
        }
    }
}
