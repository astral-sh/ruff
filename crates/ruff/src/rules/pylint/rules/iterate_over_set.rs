use rustpython_parser::ast::{Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for iterating over a `set`.
///
/// ## Why is this bad?
/// Iterating over a `set` is slower than iterating over a sequence type (such
/// as `list` or `tuple`) because `set` access is performed using a hash table.
///
/// ## Example
/// ```python
/// for item in {1, 2, 3}:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// for item in (1, 2, 3):
///     ...
/// ```
///
/// ## References
/// - TODO: add references
#[violation]
pub struct IterateOverSet;

impl Violation for IterateOverSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use a sequence type when iterating over values")
    }
}

/// PLC0208
pub(crate) fn iterate_over_set(checker: &mut Checker, stmt: &Stmt, iter: &Expr) {
    if let Expr::Set(_) = iter {
        checker.diagnostics.push(Diagnostic::new(
            IterateOverSet,
            helpers::identifier_range(stmt, checker.locator),
        ));
    }
    // TODO: check if iterating over a name that corresponds to a set (questionable).
    // TODO: check if iterating over a set in a comprehension.
}
