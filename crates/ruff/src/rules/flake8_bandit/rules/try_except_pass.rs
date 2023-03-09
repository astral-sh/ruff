use rustpython_parser::ast::{Excepthandler, Expr, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::rules::flake8_bandit::helpers::is_untyped_exception;

#[violation]
pub struct TryExceptPass;

impl Violation for TryExceptPass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`try`-`except`-`pass` detected, consider logging the exception")
    }
}

/// S110
pub fn try_except_pass(
    checker: &mut Checker,
    excepthandler: &Excepthandler,
    type_: Option<&Expr>,
    _name: Option<&str>,
    body: &[Stmt],
    check_typed_exception: bool,
) {
    if body.len() == 1
        && body[0].node == StmtKind::Pass
        && (check_typed_exception || is_untyped_exception(type_, checker))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(TryExceptPass, Range::from(excepthandler)));
    }
}
