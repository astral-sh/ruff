use rustpython_parser::ast::{self, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for `continue` statements outside of loops.
///
/// ## Why is this bad?
/// The use of a `continue` statement outside of a `for` or `while` loop will
/// raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// def foo():
///     continue  # SyntaxError
/// ```
///
/// ## References
/// - [Python documentation: `continue`](https://docs.python.org/3/reference/simple_stmts.html#the-continue-statement)
#[violation]
pub struct ContinueOutsideLoop;

impl Violation for ContinueOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`continue` not properly in loop")
    }
}

/// F702
pub(crate) fn continue_outside_loop<'a>(
    stmt: &'a Stmt,
    parents: &mut impl Iterator<Item = &'a Stmt>,
) -> Option<Diagnostic> {
    let mut allowed: bool = false;
    let mut child = stmt;
    for parent in parents {
        match parent {
            Stmt::For(ast::StmtFor { orelse, .. })
            | Stmt::AsyncFor(ast::StmtAsyncFor { orelse, .. })
            | Stmt::While(ast::StmtWhile { orelse, .. }) => {
                if !orelse.contains(child) {
                    allowed = true;
                    break;
                }
            }
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => {
                break;
            }
            _ => {}
        }
        child = parent;
    }

    if allowed {
        None
    } else {
        Some(Diagnostic::new(ContinueOutsideLoop, stmt.range()))
    }
}
