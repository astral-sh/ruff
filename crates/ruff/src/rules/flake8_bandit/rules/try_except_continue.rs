use rustpython_parser::ast::{ExceptHandler, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::flake8_bandit::helpers::is_untyped_exception;

#[violation]
pub struct TryExceptContinue;

impl Violation for TryExceptContinue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`try`-`except`-`continue` detected, consider logging the exception")
    }
}

/// S112
pub(crate) fn try_except_continue(
    checker: &mut Checker,
    except_handler: &ExceptHandler,
    type_: Option<&Expr>,
    _name: Option<&str>,
    body: &[Stmt],
    check_typed_exception: bool,
) {
    if body.len() == 1
        && body[0].is_continue_stmt()
        && (check_typed_exception || is_untyped_exception(type_, checker.semantic()))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(TryExceptContinue, except_handler.range()));
    }
}
