use rustpython_ast::{Stmt, StmtKind};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn walk_stmt(xxxxxxxx: &mut xxxxxxxx, body: &[Stmt], f: fn(&Stmt) -> bool) {
    for stmt in body {
        if f(stmt) {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::JumpStatementInFinally(match &stmt.node {
                    StmtKind::Break { .. } => "break".to_string(),
                    StmtKind::Continue { .. } => "continue".to_string(),
                    StmtKind::Return { .. } => "return".to_string(),
                    _ => unreachable!(
                        "Expected StmtKind::Break | StmtKind::Continue | StmtKind::Return"
                    ),
                }),
                Range::from_located(stmt),
            ));
        }
        match &stmt.node {
            StmtKind::While { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::AsyncFor { body, .. } => {
                walk_stmt(xxxxxxxx, body, |stmt| {
                    matches!(stmt.node, StmtKind::Return { .. })
                });
            }
            StmtKind::If { body, .. }
            | StmtKind::Try { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. } => {
                walk_stmt(xxxxxxxx, body, f);
            }
            _ => {}
        }
    }
}

/// B012
pub fn jump_statement_in_finally(xxxxxxxx: &mut xxxxxxxx, finalbody: &[Stmt]) {
    walk_stmt(xxxxxxxx, finalbody, |stmt| {
        matches!(
            stmt.node,
            StmtKind::Break | StmtKind::Continue | StmtKind::Return { .. }
        )
    });
}
