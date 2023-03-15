
use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `continue` inside of a `finally` clause
///
/// ## Why is this bad?
/// `continue` is not supported inside a `finally` clause; this can cause a SyntaxError
///
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
        format!("`continue` not supported inside a `finally` clause")
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
            StmtKind::If { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. } => {
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
