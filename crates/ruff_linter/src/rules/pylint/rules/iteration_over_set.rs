use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Expr;
use ruff_python_ast::comparable::HashableExpr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for iteration over a `set` literal in which each element is a type literal.
///
/// ## Why is this bad?
/// Evaluating a `set` literal for each iteration is less efficient than evaluating a 
/// `list` literal for each iteration, which is less efficient than the one-time 
/// evaluation of a `tuple` literal at the initialization of the iteration.
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
/// Or use instead:
/// ```python
/// set_number = {1, 2, 3}  # Not a `set` literal, but a `set` variable.
/// for number in set_number:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
/// - [Python documentation: `list` and `tuple`](https://docs.python.org/3/library/stdtypes.html#sequence-types-list-tuple-range)
/// - [Python documentation: Literal expressions](https://docs.python.org/3/reference/expressions.html#literals)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.271")]
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

    let mut diagnostic = checker.report_diagnostic(IterationOverSet, expr.range());

    let tuple = if let [elt] = set.elts.as_slice() {
        let elt = checker.locator().slice(elt);
        format!("({elt},)")
    } else {
        let set = checker.locator().slice(expr);
        format!("({})", &set[1..set.len() - 1])
    };
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(tuple, expr.range())));
}
