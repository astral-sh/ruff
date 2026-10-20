use ruff_python_ast::{self as ast, Stmt};

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Scope {
    /// Directly inside the `finally` block or a non-loop compound statement (`if`, `try`, `with`, `match`).
    Finally,
    /// Inside a loop within the `finally` block, where `break` and `continue` do not jump out of `finally`.
    Loop,
}

fn walk_stmt(checker: &Checker, body: &[Stmt], scope: Scope) {
    for stmt in body {
        let jump_name = match (stmt, scope) {
            (Stmt::Break(_), Scope::Finally) => Some("break"),
            (Stmt::Continue(_), Scope::Finally) => Some("continue"),
            (Stmt::Return(_), _) => Some("return"),
            _ => None,
        };

        if let Some(name) = jump_name {
            checker.report_diagnostic(
                JumpStatementInFinally {
                    name: (*name).to_string(),
                },
                stmt.range(),
            );
        }

        match stmt {
            Stmt::While(ast::StmtWhile { body, orelse, .. })
            | Stmt::For(ast::StmtFor { body, orelse, .. }) => {
                walk_stmt(checker, body, Scope::Loop);
                walk_stmt(checker, orelse, scope);
            }
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                walk_stmt(checker, body, scope);
                for clause in elif_else_clauses {
                    walk_stmt(checker, &clause.body, scope);
                }
            }
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                ..
            }) => {
                walk_stmt(checker, body, scope);
                for handler in handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    walk_stmt(checker, &handler.body, scope);
                }
                walk_stmt(checker, orelse, scope);
            }
            Stmt::With(ast::StmtWith { body, .. }) => {
                walk_stmt(checker, body, scope);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    walk_stmt(checker, &case.body, scope);
                }
            }
            _ => {}
        }
    }
}

/// B012
pub(crate) fn jump_statement_in_finally(checker: &Checker, finalbody: &[Stmt]) {
    walk_stmt(checker, finalbody, Scope::Finally);
}
