use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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

fn walk_stmt(checker: &Checker, body: &[Stmt], f: fn(&Stmt) -> bool) {
    for stmt in body {
        if f(stmt) {
            checker.report_diagnostic(Diagnostic::new(
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
            Stmt::While(ast::StmtWhile { body, .. }) | Stmt::For(ast::StmtFor { body, .. }) => {
                walk_stmt(checker, body, Stmt::is_return_stmt);
            }
            Stmt::If(ast::StmtIf { body, .. })
            | Stmt::Try(ast::StmtTry { body, .. })
            | Stmt::With(ast::StmtWith { body, .. }) => {
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
pub(crate) fn jump_statement_in_finally(checker: &Checker, finalbody: &[Stmt]) {
    walk_stmt(checker, finalbody, |stmt| {
        matches!(stmt, Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_))
    });
}
