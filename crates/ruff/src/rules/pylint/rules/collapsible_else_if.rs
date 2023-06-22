use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

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
pub(crate) fn collapsible_else_if(orelse: &[Stmt], locator: &Locator) -> Option<Diagnostic> {
    if orelse.len() == 1 {
        let first = &orelse[0];
        if matches!(first, Stmt::If(_)) {
            // Determine whether this is an `elif`, or an `if` in an `else` block.
            if locator.slice(first.range()).starts_with("if") {
                return Some(Diagnostic::new(CollapsibleElseIf, first.range()));
            }
        }
    }
    None
}
