use rustpython_parser::ast::{self, Ranged, Stmt};

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
                    name: match stmt {
                        Stmt::Break(_) => "break",
                        Stmt::Continue(_) => "continue",
                        Stmt::Return(_) => "return",
                        _ => unreachable!("Expected Stmt::Break | Stmt::Continue | Stmt::Return"),
                    }
                    .to_owned(),
                },
                stmt.range(),
            ));
        }
        match stmt {
            Stmt::While(ast::StmtWhile { body, .. })
            | Stmt::For(ast::StmtFor { body, .. })
            | Stmt::AsyncFor(ast::StmtAsyncFor { body, .. }) => {
                walk_stmt(checker, body, Stmt::is_return_stmt);
            }
            Stmt::If(ast::StmtIf { body, .. })
            | Stmt::Try(ast::StmtTry { body, .. })
            | Stmt::TryStar(ast::StmtTryStar { body, .. })
            | Stmt::With(ast::StmtWith { body, .. })
            | Stmt::AsyncWith(ast::StmtAsyncWith { body, .. }) => {
                walk_stmt(checker, body, f);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
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
        matches!(stmt, Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_))
    });
}
