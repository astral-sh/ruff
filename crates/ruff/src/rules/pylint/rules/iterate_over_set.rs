use rustpython_parser::ast::{Expr, ExprName, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for code that iterates over an in-place `set`.
///
/// ## Why is this bad?
/// Iterating over a `set` is slower than iterating over a sequence type (such
/// as `list` or `tuple`) because `set` access is performed using a hash
/// function, whereas sequenced items are accessed using an index directly.
///
/// ## Example
/// ```python
/// for number in {1, 2, 3}:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// for number in (1, 2, 3):
///     ...
/// ```
///
/// ## References
/// - [Python documentation](https://docs.python.org/3/library/stdtypes.html#set)
/// - [Python documentation](https://docs.python.org/3/library/stdtypes.html?highlight=list#sequence-types-list-tuple-range)
#[violation]
pub struct IterateOverSet;

impl Violation for IterateOverSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a sequence type instead of a set when iterating over values")
    }
}

/// PLC0208
pub(crate) fn iterate_over_set(checker: &mut Checker, iter: &Expr) {
    match iter {
        // Check if set literal; e.g., `{1, 2, 3}`.
        Expr::Set(_) => {
            checker
                .diagnostics
                .push(Diagnostic::new(IterateOverSet, iter.range()));
        }
        // Check if call to set constructor; e.g., `set(1, 2, 3)`.
        Expr::Call(call) => {
            if let Expr::Name(ExprName { id, .. }) = &*call.func {
                if id.as_str() == "set" {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(IterateOverSet, iter.range()));
                }
            }
        }
        // Check if set comprehension; e.g., `{n for n in range(1, 4)}`.
        Expr::SetComp(_) => {
            checker
                .diagnostics
                .push(Diagnostic::new(IterateOverSet, iter.range()));
        }
        _ => {}
    }
}
