use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `assert` statements that use non-empty tuples as test
/// conditions.
///
/// ## Why is this bad?
/// Non-empty tuples are always `True`, so an `assert` statement with a
/// non-empty tuple as its test condition will always pass. This is likely a
/// mistake.
///
/// ## Example
/// ```python
/// assert (some_condition,)
/// ```
///
/// Use instead:
/// ```python
/// assert some_condition
/// ```
///
/// ## References
/// - [Python documentation: The `assert` statement](https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement)
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
