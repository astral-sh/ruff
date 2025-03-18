use ruff_python_ast::{self as ast};
use ruff_python_ast::{ExceptHandler, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for exception handlers that catch an empty tuple.
///
/// ## Why is this bad?
/// An exception handler that catches an empty tuple will not catch anything,
/// and is indicative of a mistake. Instead, add exceptions to the `except`
/// clause.
///
/// ## Example
/// ```python
/// try:
///     1 / 0
/// except ():
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     1 / 0
/// except ZeroDivisionError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[derive(ViolationMetadata)]
pub(crate) struct ExceptWithEmptyTuple {
    is_star: bool,
}

impl Violation for ExceptWithEmptyTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_star {
            "Using `except* ():` with an empty tuple does not catch anything; add exceptions to handle".to_string()
        } else {
            "Using `except ():` with an empty tuple does not catch anything; add exceptions to handle".to_string()
        }
    }
}

/// B029
pub(crate) fn except_with_empty_tuple(checker: &Checker, except_handler: &ExceptHandler) {
    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) =
        except_handler;
    let Some(type_) = type_ else {
        return;
    };
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = type_.as_ref() else {
        return;
    };

    if elts.is_empty() {
        let is_star = checker
            .semantic()
            .current_statement()
            .as_try_stmt()
            .is_some_and(|try_stmt| try_stmt.is_star);
        checker.report_diagnostic(Diagnostic::new(
            ExceptWithEmptyTuple { is_star },
            except_handler.range(),
        ));
    }
}
