use rustpython_parser::ast::{Expr, ExprName, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for iterations over `set` literals and comprehensions.
///
/// ## Why is this bad?
/// Iterating over a `set` is less efficient than iterating over a sequence
/// type, like `list` or `tuple`.
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
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[violation]
pub struct SetIteration;

impl Violation for SetIteration {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a sequence type instead of a `set` when iterating over values")
    }
}

/// PLC0208
pub(crate) fn set_iteration(checker: &mut Checker, expr: &Expr) {
    let is_set = match expr {
        // Ex) `for i in {1, 2, 3}`
        Expr::Set(_) => true,
        // Ex)` for i in {n for n in range(1, 4)}`
        Expr::SetComp(_) => true,
        // Ex) `for i in set(1, 2, 3)`
        Expr::Call(call) => {
            if let Expr::Name(ExprName { id, .. }) = call.func.as_ref() {
                id.as_str() == "set" && checker.semantic_model().is_builtin("set")
            } else {
                false
            }
        }
        _ => false,
    };

    if is_set {
        checker
            .diagnostics
            .push(Diagnostic::new(SetIteration, expr.range()));
    }
}
