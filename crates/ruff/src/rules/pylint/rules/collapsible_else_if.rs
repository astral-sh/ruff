use ruff_python_ast::{ElifElseClause, Stmt};
use ruff_text_size::{Ranged, TextRange};

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
    let Some(ElifElseClause {
        body,
        test: None,
        range,
    }) = elif_else_clauses.last()
    else {
        return None;
    };
    if let [first @ Stmt::If(_)] = body.as_slice() {
        return Some(Diagnostic::new(
            CollapsibleElseIf,
            TextRange::new(range.start(), first.start()),
        ));
    }
    None
}
