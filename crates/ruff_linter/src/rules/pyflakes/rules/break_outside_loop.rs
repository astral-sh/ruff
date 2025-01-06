use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for `break` statements outside of loops.
///
/// ## Why is this bad?
/// The use of a `break` statement outside of a `for` or `while` loop will
/// raise a `SyntaxError`.
///
/// ## Example
/// ```python
/// def foo():
///     break
/// ```
///
/// ## References
/// - [Python documentation: `break`](https://docs.python.org/3/reference/simple_stmts.html#the-break-statement)
#[derive(ViolationMetadata)]
pub(crate) struct BreakOutsideLoop;

impl Violation for BreakOutsideLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`break` outside loop".to_string()
    }
}

/// F701
pub(crate) fn break_outside_loop<'a>(
    stmt: &'a Stmt,
    parents: &mut impl Iterator<Item = &'a Stmt>,
) -> Option<Diagnostic> {
    let mut child = stmt;
    for parent in parents {
        match parent {
            Stmt::For(ast::StmtFor { orelse, .. }) | Stmt::While(ast::StmtWhile { orelse, .. }) => {
                if !orelse.contains(child) {
                    return None;
                }
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                break;
            }
            _ => {}
        }
        child = parent;
    }

    Some(Diagnostic::new(BreakOutsideLoop, stmt.range()))
}
