use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::HashableExpr;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for iteration over a `set` literal where each element in the set is
/// itself a literal value.
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
#[derive(ViolationMetadata)]
pub(crate) struct IterationOverSet;

impl AlwaysFixableViolation for IterationOverSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use a sequence type instead of a `set` when iterating over values".to_string()
    }

    fn fix_title(&self) -> String {
        "Convert to `tuple`".to_string()
    }
}

/// PLC0208
pub(crate) fn iteration_over_set(checker: &Checker, expr: &Expr) {
    let Expr::Set(set) = expr else {
        return;
    };

    if set.iter().any(|value| !value.is_literal_expr()) {
        return;
    }

    let mut seen_values = FxHashSet::with_capacity_and_hasher(set.len(), FxBuildHasher);
    for value in set {
        if !seen_values.insert(HashableExpr::from(value)) {
            // if the set contains a duplicate literal value, early exit.
            // rule `B033` can catch that.
            return;
        }
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

    checker.report_diagnostic(diagnostic);
}
