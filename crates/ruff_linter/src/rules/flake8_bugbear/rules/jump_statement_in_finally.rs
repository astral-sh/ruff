use ruff_python_ast::{self as ast, Stmt, StmtKind, Suite};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for `break`, `continue`, and `return` statements in `finally`
/// blocks.
///
/// ## Why is this bad?
/// The use of `break`, `continue`, and `return` statements in `finally` blocks
/// can cause exceptions to be silenced.
///
/// `finally` blocks execute regardless of whether an exception is raised. If a
/// `break`, `continue`, or `return` statement is reached in a `finally` block,
/// any exception raised in the `try` or `except` blocks will be silenced.
///
/// ## Example
/// ```python
/// def speed(distance, time):
///     try:
///         return distance / time
///     except ZeroDivisionError:
///         raise ValueError("Time cannot be zero")
///     finally:
///         return 299792458  # `ValueError` is silenced
/// ```
///
/// Use instead:
/// ```python
/// def speed(distance, time):
///     try:
///         return distance / time
///     except ZeroDivisionError:
///         raise ValueError("Time cannot be zero")
/// ```
///
/// ## References
/// - [Python documentation: The `try` statement](https://docs.python.org/3/reference/compound_stmts.html#the-try-statement)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.116", category = Category::Suspicious)]
pub(crate) struct JumpStatementInFinally {
    name: String,
}

impl Violation for JumpStatementInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        let JumpStatementInFinally { name } = self;
        format!("`{name}` inside `finally` blocks cause exceptions to be silenced")
    }
}

fn walk_stmt<'a>(checker: &Checker, body: Suite<'a>, f: fn(Stmt<'a>) -> bool) {
    for stmt in body {
        if f(stmt) {
            checker.report_diagnostic(
                JumpStatementInFinally {
                    name: match stmt.classify() {
                        StmtKind::Break(_) => "break",
                        StmtKind::Continue(_) => "continue",
                        StmtKind::Return(_) => "return",
                        _ => unreachable!("Expected Stmt::Break | Stmt::Continue | Stmt::Return"),
                    }
                    .to_owned(),
                },
                stmt.range(),
            );
        }
        match stmt.fields() {
            ast::StmtFields::While(ast::StmtWhileFields { body, .. })
            | ast::StmtFields::For(ast::StmtForFields { body, .. }) => {
                walk_stmt(checker, body, ruff_python_ast::Stmt::is_return_stmt);
            }
            ast::StmtFields::If(ast::StmtIfFields { body, .. })
            | ast::StmtFields::Try(ast::StmtTryFields { body, .. })
            | ast::StmtFields::With(ast::StmtWithFields { body, .. }) => {
                walk_stmt(checker, body, f);
            }
            ast::StmtFields::Match(ast::StmtMatchFields { cases, .. }) => {
                for case in cases {
                    walk_stmt(checker, case.body(), f);
                }
            }
            _ => {}
        }
    }
}

/// B012
pub(crate) fn jump_statement_in_finally(checker: &Checker, finalbody: Suite<'_>) {
    walk_stmt(checker, finalbody, |stmt| {
        matches!(
            stmt.classify(),
            StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Return(_)
        )
    });
}
