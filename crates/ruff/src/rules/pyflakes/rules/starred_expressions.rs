use ruff_text_size::TextRange;
use rustpython_parser::ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

#[violation]
pub struct ExpressionsInStarAssignment;

impl Violation for ExpressionsInStarAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many expressions in star-unpacking assignment")
    }
}

/// ## What it does
/// Checks for more than one starred expression in an assignment.
///
/// ## Why is this bad?
/// Starred expressions in assignments are used to unpack iterables. If there
/// are more than one starred expressions, it is unclear how the iterable is
/// unpacked and will raise a `SyntaxError` at runtime.
///
/// ## Example
/// ```python
/// *foo, *bar, baz = (1, 2, 3)
/// ```
///
/// Use instead:
/// ```python
/// *foo, bar, baz = (1, 2, 3)
/// ```
///
/// ## References
/// - [PEP 3132](https://peps.python.org/pep-3132/)
#[violation]
pub struct MultipleStarredExpressions;

impl Violation for MultipleStarredExpressions {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Two starred expressions in assignment")
    }
}

/// F621, F622
pub(crate) fn starred_expressions(
    elts: &[Expr],
    check_too_many_expressions: bool,
    check_two_starred_expressions: bool,
    location: TextRange,
) -> Option<Diagnostic> {
    let mut has_starred: bool = false;
    let mut starred_index: Option<usize> = None;
    for (index, elt) in elts.iter().enumerate() {
        if matches!(elt, Expr::Starred(_)) {
            if has_starred && check_two_starred_expressions {
                return Some(Diagnostic::new(MultipleStarredExpressions, location));
            }
            has_starred = true;
            starred_index = Some(index);
        }
    }

    if check_too_many_expressions {
        if let Some(starred_index) = starred_index {
            if starred_index >= 1 << 8 || elts.len() - starred_index > 1 << 24 {
                return Some(Diagnostic::new(ExpressionsInStarAssignment, location));
            }
        }
    }

    None
}
