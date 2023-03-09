use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct JumpStatementInFinally {
    pub name: String,
}

impl Violation for JumpStatementInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        let JumpStatementInFinally { name } = self;
        format!("`{name}` inside `finally` blocks cause exceptions to be silenced")
    }
}

fn walk_stmt(checker: &mut Checker, body: &[Stmt], f: fn(&Stmt) -> bool) {
    for stmt in body {
        if f(stmt) {
            checker.diagnostics.push(Diagnostic::new(
                JumpStatementInFinally {
                    name: match &stmt.node {
                        StmtKind::Break { .. } => "break".to_string(),
                        StmtKind::Continue { .. } => "continue".to_string(),
                        StmtKind::Return { .. } => "return".to_string(),
                        _ => unreachable!(
                            "Expected StmtKind::Break | StmtKind::Continue | StmtKind::Return"
                        ),
                    },
                },
                Range::from(stmt),
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
            | StmtKind::TryStar { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. } => {
                walk_stmt(checker, body, f);
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    walk_stmt(checker, &case.body, f);
                }
            }
            _ => {}
        }
    }
}

/// B012
pub fn jump_statement_in_finally(checker: &mut Checker, finalbody: &[Stmt]) {
    walk_stmt(checker, finalbody, |stmt| {
        matches!(
            stmt.node,
            StmtKind::Break | StmtKind::Continue | StmtKind::Return { .. }
        )
    });
}
