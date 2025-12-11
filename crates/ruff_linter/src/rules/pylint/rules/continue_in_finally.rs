use ruff_python_ast::Stmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
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
#[violation_metadata(stable_since = "v0.0.257")]
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
            checker.report_diagnostic(ContinueInFinally, stmt.range());
        }

        match stmt {
            Stmt::If(if_stmt) => {
                traverse_body(checker, &if_stmt.body);
                for clause in &if_stmt.elif_else_clauses {
                    traverse_body(checker, &clause.body);
                }
            }
            Stmt::Try(try_stmt) => {
                traverse_body(checker, &try_stmt.body);
                traverse_body(checker, &try_stmt.orelse);
            }
            Stmt::For(for_stmt) => {
                traverse_body(checker, &for_stmt.orelse);
            }
            Stmt::While(while_stmt) => {
                traverse_body(checker, &while_stmt.orelse);
            }
            Stmt::With(with_stmt) => {
                traverse_body(checker, &with_stmt.body);
            }
            Stmt::Match(match_stmt) => {
                for case in &match_stmt.cases {
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
