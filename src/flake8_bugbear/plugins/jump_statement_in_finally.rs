use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

fn walk_stmt(checker: &mut Checker, body: &Vec<Stmt>, f: fn(&Stmt) -> bool) {
    for stmt in body {
        if f(stmt) {
            checker.add_check(Check::new(
                CheckKind::JumpStatementInFinally,
                Range::from_located(stmt),
            ));
        }
        match &stmt.node {
            StmtKind::While { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::AsyncFor { body, .. } => {
                walk_stmt(checker, body, |stmt| {
                    matches!(stmt.node, StmtKind::Return { .. })
                });
            }
            StmtKind::If { body, .. }
            | StmtKind::Try { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. } => {
                walk_stmt(checker, body, f);
            }
            _ => {}
        }
    }
}

/// B012
pub fn jump_statement_in_finally(checker: &mut Checker, finalbody: &Vec<Stmt>) {
    walk_stmt(checker, finalbody, |stmt| {
        matches!(
            stmt.node,
            StmtKind::Break | StmtKind::Continue | StmtKind::Return { .. }
        )
    });
}
