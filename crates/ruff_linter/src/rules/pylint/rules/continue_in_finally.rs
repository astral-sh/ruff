use ruff_python_ast::{self as ast, Suite};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "v0.0.257", category = Category::Correctness)]
pub(crate) struct ContinueInFinally;

impl Violation for ContinueInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`continue` not supported inside `finally` clause".to_string()
    }
}

fn traverse_body(checker: &Checker, body: Suite<'_>) {
    for stmt in body {
        if stmt.is_continue_stmt() {
            checker.report_diagnostic(ContinueInFinally, stmt.range());
        }

        match stmt.fields() {
            ast::StmtFields::If(ast::StmtIfFields {
                body,
                elif_else_clauses,
                ..
            }) => {
                traverse_body(checker, body);
                for clause in elif_else_clauses {
                    traverse_body(checker, clause.body());
                }
            }
            ast::StmtFields::Try(ast::StmtTryFields { body, orelse, .. }) => {
                traverse_body(checker, body);
                traverse_body(checker, orelse);
            }
            ast::StmtFields::For(ast::StmtForFields { orelse, .. })
            | ast::StmtFields::While(ast::StmtWhileFields { orelse, .. }) => {
                traverse_body(checker, orelse);
            }
            ast::StmtFields::With(ast::StmtWithFields { body, .. }) => {
                traverse_body(checker, body);
            }
            ast::StmtFields::Match(ast::StmtMatchFields { cases, .. }) => {
                for case in cases {
                    traverse_body(checker, case.body());
                }
            }
            _ => {}
        }
    }
}

/// PLE0116
pub(crate) fn continue_in_finally(checker: &Checker, body: Suite<'_>) {
    traverse_body(checker, body);
}
