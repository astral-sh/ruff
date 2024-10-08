use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
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

impl AlwaysFixableViolation for IterationOverSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a sequence type instead of a `set` when iterating over values")
    }

    fn fix_title(&self) -> String {
        format!("Convert to `tuple`")
    }
}

/// PLC0208
pub(crate) fn iteration_over_set(checker: &mut Checker, expr: &Expr) {
    let Expr::Set(set) = expr else {
        return;
    };

    if set.iter().any(Expr::is_starred_expr) {
        return;
    }

    let mut diagnostic = Diagnostic::new(IterationOverSet, expr.range());

    let tuple = if let [elt] = set.elts.as_slice() {
        let elt = checker.locator().slice(elt);
        format!("({elt},)")
    } else {
        let set = checker.locator().slice(expr);
        format!("({})", &set[1..set.len() - 1])
    };
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(tuple, expr.range())));

    checker.diagnostics.push(diagnostic);
}
