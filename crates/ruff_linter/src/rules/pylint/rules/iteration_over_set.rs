use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for iterations over `set` literals.
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
pub struct IterationOverSet;

impl Violation for IterationOverSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a sequence type instead of a `set` when iterating over values")
    }
}

/// PLC0208
pub(crate) fn iteration_over_set(checker: &mut Checker, expr: &Expr) {
    let Expr::Set(ast::ExprSet { elts, .. }) = expr else {
        return;
    };

    if elts.iter().any(Expr::is_starred_expr) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(IterationOverSet, expr.range()));
}
