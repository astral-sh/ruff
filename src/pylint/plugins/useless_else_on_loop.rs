use rustpython_ast::{ExcepthandlerKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

fn loop_exits_early(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match &stmt.node {
        StmtKind::If { body, orelse, .. } => loop_exits_early(body) || loop_exits_early(orelse),
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        } => {
            loop_exits_early(body)
                || loop_exits_early(orelse)
                || loop_exits_early(finalbody)
                || handlers.iter().any(|handler| match &handler.node {
                    ExcepthandlerKind::ExceptHandler { body, .. } => loop_exits_early(body),
                })
        }
        StmtKind::For { orelse, .. }
        | StmtKind::AsyncFor { orelse, .. }
        | StmtKind::While { orelse, .. } => loop_exits_early(orelse),
        StmtKind::Break { .. } => true,
        _ => false,
    })
}

/// PLW0120
pub fn useless_else_on_loop(checker: &mut Checker, stmt: &Stmt, body: &[Stmt], orelse: &[Stmt]) {
    if !orelse.is_empty() && !loop_exits_early(body) {
        checker.add_check(Check::new(
            CheckKind::UselessElseOnLoop,
            Range::from_located(stmt),
        ));
    }
}
