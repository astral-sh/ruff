use ruff_python_ast::Stmt;
use ruff_python_ast::flat::{self, fields as ast};

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

fn walk_stmt<'a>(checker: &Checker, body: flat::Suite<'a>, f: fn(flat::View<'a, Stmt>) -> bool) {
    for stmt in body {
        if f(stmt) {
            checker.report_diagnostic(
                JumpStatementInFinally {
                    name: match stmt.classify() {
                        flat::StmtKind::Break(_) => "break",
                        flat::StmtKind::Continue(_) => "continue",
                        flat::StmtKind::Return(_) => "return",
                        _ => unreachable!("Expected Stmt::Break | Stmt::Continue | Stmt::Return"),
                    }
                    .to_owned(),
                },
                stmt.range(),
            );
        }
        match stmt.fields() {
            ast::Stmt::While(ast::StmtWhile { body, .. })
            | ast::Stmt::For(ast::StmtFor { body, .. }) => {
                walk_stmt(checker, body, flat::View::is_return_stmt);
            }
            ast::Stmt::If(ast::StmtIf { body, .. })
            | ast::Stmt::Try(ast::StmtTry { body, .. })
            | ast::Stmt::With(ast::StmtWith { body, .. }) => {
                walk_stmt(checker, body, f);
            }
            ast::Stmt::Match(ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    walk_stmt(checker, case.body(), f);
                }
            }
            _ => {}
        }
    }
}

/// B012
pub(crate) fn jump_statement_in_finally(checker: &Checker, finalbody: flat::Suite<'_>) {
    walk_stmt(checker, finalbody, |stmt| {
        matches!(
            stmt.classify(),
            flat::StmtKind::Break(_) | flat::StmtKind::Continue(_) | flat::StmtKind::Return(_)
        )
    });
}
