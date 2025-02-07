use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

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
///     try:
///         pass
///     finally:
///         continue
/// ```
///
/// Use instead:
/// ```python
/// while True:
///     try:
///         pass
///     except Exception:
///         pass
///     else:
///         continue
/// ```
///
/// ## Options
/// - `target-version`
#[derive(ViolationMetadata)]
pub(crate) struct ContinueInFinally;

impl Violation for ContinueInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`continue` not supported inside `finally` clause".to_string()
    }
}

fn traverse_body(checker: &Checker, body: &[Stmt]) {
    for stmt in body {
        if stmt.is_continue_stmt() {
            checker.report_diagnostic(Diagnostic::new(ContinueInFinally, stmt.range()));
        }

        match stmt {
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                traverse_body(checker, body);
                for clause in elif_else_clauses {
                    traverse_body(checker, &clause.body);
                }
            }
            Stmt::Try(ast::StmtTry { body, orelse, .. }) => {
                traverse_body(checker, body);
                traverse_body(checker, orelse);
            }
            Stmt::For(ast::StmtFor { orelse, .. }) | Stmt::While(ast::StmtWhile { orelse, .. }) => {
                traverse_body(checker, orelse);
            }
            Stmt::With(ast::StmtWith { body, .. }) => {
                traverse_body(checker, body);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    traverse_body(checker, &case.body);
                }
            }
            _ => {}
        }
    }
}

/// PLE0116
pub(crate) fn continue_in_finally(checker: &Checker, body: &[Stmt]) {
    traverse_body(checker, body);
}
