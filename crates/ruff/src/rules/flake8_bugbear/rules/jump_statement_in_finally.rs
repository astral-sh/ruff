use rustpython_parser::ast::{self, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[violation]
pub struct JumpStatementInFinally {
    name: String,
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
                        StmtKind::Break => "break",
                        StmtKind::Continue => "continue",
                        StmtKind::Return(_) => "return",
                        _ => unreachable!(
                            "Expected StmtKind::Break | StmtKind::Continue | StmtKind::Return"
                        ),
                    }
                    .to_owned(),
                },
                stmt.range(),
            ));
        }
        match &stmt.node {
            StmtKind::While(ast::StmtWhile { body, .. })
            | StmtKind::For(ast::StmtFor { body, .. })
            | StmtKind::AsyncFor(ast::StmtAsyncFor { body, .. }) => {
                walk_stmt(checker, body, |stmt| {
                    matches!(stmt.node, StmtKind::Return(_))
                });
            }
            StmtKind::If(ast::StmtIf { body, .. })
            | StmtKind::Try(ast::StmtTry { body, .. })
            | StmtKind::TryStar(ast::StmtTryStar { body, .. })
            | StmtKind::With(ast::StmtWith { body, .. })
            | StmtKind::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
                walk_stmt(checker, body, f);
            }
            StmtKind::Match(ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    walk_stmt(checker, &case.body, f);
                }
            }
            _ => {}
        }
    }
}

/// B012
pub(crate) fn jump_statement_in_finally(checker: &mut Checker, finalbody: &[Stmt]) {
    walk_stmt(checker, finalbody, |stmt| {
        matches!(
            stmt.node,
            StmtKind::Break | StmtKind::Continue | StmtKind::Return(_)
        )
    });
}
