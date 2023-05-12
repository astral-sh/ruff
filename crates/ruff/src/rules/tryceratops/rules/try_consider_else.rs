use rustpython_parser::ast::{self, Excepthandler, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `return` statements in `try` blocks.
///
/// ## Why is this bad?
/// The `try`-`except` statement has an `else` clause for code that should
/// run _only_ if no exceptions were raised. Using the `else` clause is more
/// explicit than using a `return` statement inside of a `try` block.
///
/// ## Example
/// ```python
/// import logging
///
///
/// def reciprocal(n):
///     try:
///         rec = 1 / n
///         print(f"reciprocal of {n} is {rec}")
///         return rec
///     except ZeroDivisionError as exc:
///         logging.exception("Exception occurred")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
///
/// def reciprocal(n):
///     try:
///         rec = 1 / n
///     except ZeroDivisionError as exc:
///         logging.exception("Exception occurred")
///     else:
///         print(f"reciprocal of {n} is {rec}")
///         return rec
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/tutorial/errors.html)
#[violation]
pub struct TryConsiderElse;

impl Violation for TryConsiderElse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider moving this statement to an `else` block")
    }
}

/// TRY300
pub(crate) fn try_consider_else(
    checker: &mut Checker,
    body: &[Stmt],
    orelse: &[Stmt],
    handler: &[Excepthandler],
) {
    if body.len() > 1 && orelse.is_empty() && !handler.is_empty() {
        if let Some(stmt) = body.last() {
            if let StmtKind::Return(ast::StmtReturn { value }) = &stmt.node {
                if let Some(value) = value {
                    if contains_effect(value, |id| checker.ctx.is_builtin(id)) {
                        return;
                    }
                }
                checker
                    .diagnostics
                    .push(Diagnostic::new(TryConsiderElse, stmt.range()));
            }
        }
    }
}
