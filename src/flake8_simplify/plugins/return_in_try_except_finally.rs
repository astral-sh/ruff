use rustpython_ast::{Excepthandler, ExcepthandlerKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

fn find_return(stmts: &[Stmt]) -> Option<&Stmt> {
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
                violations::ReturnInTryExceptFinally,
                Range::from_located(finally_return),
            ));
        }
    }
}
