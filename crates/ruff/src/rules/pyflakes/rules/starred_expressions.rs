use ruff_python_ast::Expr;
use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for the use of too many expressions in starred assignment statements.
///
/// ## Why is this bad?
/// In assignment statements, starred expressions can be used to unpack iterables.
///
/// In Python 3, no more than 1 << 8 assignments are allowed before a starred
/// expression, and no more than 1 << 24 expressions are allowed after a starred
/// expression.
#[violation]
pub struct ExpressionsInStarAssignment;

impl Violation for ExpressionsInStarAssignment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many expressions in star-unpacking assignment")
    }
}

/// ## What it does
/// Checks for the use of multiple starred expressions in assignment statements.
///
/// ## Why is this bad?
/// In assignment statements, starred expressions can be used to unpack iterables.
/// Including more than one starred expression on the left-hand-side of an
/// assignment will cause a `SyntaxError`, as it is unclear which expression
/// should receive the remaining values.
///
/// ## Example
/// ```python
/// *foo, *bar, baz = (1, 2, 3)
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
        if elt.is_starred_expr() {
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
