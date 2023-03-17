use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `continue` statements inside `finally`
///
/// ## Why is this bad?
/// `continue` statements were not allowed within `finally` clauses prior to
/// Python 3.8. Using a `continue` statement within a `finally` clause can
/// cause a `SyntaxError`.
///
/// ## Example
/// ```python
/// while True:
///  try:
///     pass
///  finally:
///     continue
/// ```
///
/// Use instead:
/// ```python
/// while True:
///  try:
///     pass
///  except Exception:
///     pass
///  else:
///     continue
/// ```
#[violation]
pub struct ContinueInFinally;

impl Violation for ContinueInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`continue` not supported inside `finally` clause")
    }
}

fn traverse_body(checker: &mut Checker, body: &[Stmt]) {
    for stmt in body {
        if matches!(stmt.node, StmtKind::Continue { .. }) {
            checker
                .diagnostics
                .push(Diagnostic::new(ContinueInFinally, Range::from(stmt)));
        }

        match &stmt.node {
            StmtKind::If { body, orelse, .. }
            | StmtKind::Try { body, orelse, .. }
            | StmtKind::TryStar { body, orelse, .. } => {
                traverse_body(checker, body);
                traverse_body(checker, orelse);
            }
            StmtKind::For { orelse, .. }
            | StmtKind::AsyncFor { orelse, .. }
            | StmtKind::While { orelse, .. } => traverse_body(checker, orelse),
            StmtKind::With { body, .. } | StmtKind::AsyncWith { body, .. } => {
                traverse_body(checker, body);
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    traverse_body(checker, &case.body);
                }
            }
            _ => {}
        }
    }
}

/// PLE0116
pub fn continue_in_finally(checker: &mut Checker, body: &[Stmt]) {
    traverse_body(checker, body);
}
