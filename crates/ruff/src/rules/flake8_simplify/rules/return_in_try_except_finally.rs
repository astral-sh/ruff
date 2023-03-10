use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct ReturnInTryExceptFinally;

impl Violation for ReturnInTryExceptFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't use `return` in `try`/`except` and `finally`")
    }
}

fn find_return(stmts: &[rustpython_parser::ast::Stmt]) -> Option<&Stmt> {
    stmts
        .iter()
        .find(|stmt| matches!(stmt.node, StmtKind::Return { .. }))
}

/// SIM107
pub fn return_in_try_except_finally(
    checker: &mut Checker,
    body: &[Stmt],
    handlers: &[Excepthandler],
    finalbody: &[Stmt],
) {
    let try_has_return = find_return(body).is_some();
    let except_has_return = handlers.iter().any(|handler| {
        let ExcepthandlerKind::ExceptHandler { body, .. } = &handler.node;
        find_return(body).is_some()
    });

    if let Some(finally_return) = find_return(finalbody) {
        if try_has_return || except_has_return {
            checker.diagnostics.push(Diagnostic::new(
                ReturnInTryExceptFinally,
                Range::from(finally_return),
            ));
        }
    }
}
