use rustpython_parser::ast::{self, ExcepthandlerKind, MatchCase, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;

use crate::checkers::ast::Checker;

#[violation]
pub struct UselessElseOnLoop;

impl Violation for UselessElseOnLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`else` clause on loop without a `break` statement; remove the `else` and de-indent all the \
             code inside it"
        )
    }
}

fn loop_exits_early(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match &stmt.node {
        StmtKind::If(ast::StmtIf { body, orelse, .. }) => {
            loop_exits_early(body) || loop_exits_early(orelse)
        }
        StmtKind::With(ast::StmtWith { body, .. })
        | StmtKind::AsyncWith(ast::StmtAsyncWith { body, .. }) => loop_exits_early(body),
        StmtKind::Match(ast::StmtMatch { cases, .. }) => cases
            .iter()
            .any(|MatchCase { body, .. }| loop_exits_early(body)),
        StmtKind::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
        })
        | StmtKind::TryStar(ast::StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
        }) => {
            loop_exits_early(body)
                || loop_exits_early(orelse)
                || loop_exits_early(finalbody)
                || handlers.iter().any(|handler| match &handler.node {
                    ExcepthandlerKind::ExceptHandler(ast::ExcepthandlerExceptHandler {
                        body,
                        ..
                    }) => loop_exits_early(body),
                })
        }
        StmtKind::For(ast::StmtFor { orelse, .. })
        | StmtKind::AsyncFor(ast::StmtAsyncFor { orelse, .. })
        | StmtKind::While(ast::StmtWhile { orelse, .. }) => loop_exits_early(orelse),
        StmtKind::Break => true,
        _ => false,
    })
}

/// PLW0120
pub fn useless_else_on_loop(checker: &mut Checker, stmt: &Stmt, body: &[Stmt], orelse: &[Stmt]) {
    if !orelse.is_empty() && !loop_exits_early(body) {
        checker.diagnostics.push(Diagnostic::new(
            UselessElseOnLoop,
            helpers::else_range(stmt, checker.locator).unwrap(),
        ));
    }
}
