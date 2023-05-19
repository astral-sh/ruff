use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

/// ## What it does
/// Checks for `else` blocks that contain an `if` block that can be collapsed
/// into an `elif` block.
///
/// ## Why is this bad?
/// The `else` block can be collapsed into the `if` block, reducing the
/// indentation level by one.
///
/// ## Example
/// ```python
/// def check_age(age):
///     if age >= 18:
///         print("You are old enough!")
///     else:
///         if age == 17:
///             print("You are seventeen, almost there!")
///         else:
///             print("You are not old enough.")
/// ```
///
/// Use instead:
/// ```python
/// def check_age(age):
///     if age >= 18:
///         print("You are old enough!")
///     elif age == 17:
///         print("You are seventeen, almost there!")
///     else:
///         print("You are not old enough.")
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/tutorial/controlflow.html#if-statements)
#[violation]
pub struct CollapsibleElseIf;

impl Violation for CollapsibleElseIf {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Consider using `elif` instead of `else` then `if` to remove one indentation level")
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
