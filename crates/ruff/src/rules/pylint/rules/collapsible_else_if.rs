use rustpython_parser::ast::{ElifElseClause, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for `else` blocks that consist of a single `if` statement.
///
/// ## Why is this bad?
/// If an `else` block contains a single `if` statement, it can be collapsed
/// into an `elif`, thus reducing the indentation level.
///
/// ## Example
/// ```python
/// def check_sign(value: int) -> None:
///     if value > 0:
///         print("Number is positive.")
///     else:
///         if value < 0:
///             print("Number is negative.")
///         else:
///             print("Number is zero.")
/// ```
///
/// Use instead:
/// ```python
/// def check_sign(value: int) -> None:
///     if value > 0:
///         print("Number is positive.")
///     elif value < 0:
///         print("Number is negative.")
///     else:
///         print("Number is zero.")
/// ```
///
/// ## References
/// - [Python documentation: `if` Statements](https://docs.python.org/3/tutorial/controlflow.html#if-statements)
#[violation]
pub struct CollapsibleElseIf;

impl Violation for CollapsibleElseIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `elif` instead of `else` then `if`, to reduce indentation")
    }
}

/// PLR5501
pub(crate) fn collapsible_else_if(elif_else_clauses: &[ElifElseClause]) -> Option<Diagnostic> {
    // `test: None` to test for an `else` clause
    let [ElifElseClause { body, test: None, ..}] = elif_else_clauses else {
        return None;
    };

    let first = body.first()?;
    if matches!(first, Stmt::If(_)) {
        return Some(Diagnostic::new(CollapsibleElseIf, first.range()));
    }
    None
}
