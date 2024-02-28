use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::CmpOp;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for non-transitive compound comparisons.
///
/// ## Why is this bad?
/// Compound comparisons chain comparisons arbitrarily. For example,
/// `a < b < c` is equivalent to `a < b and b < c` (with the exception that `b`
/// is only evaluated once). This can lead to unexpected behavior with some
/// combinations of comparisons.
///
/// ## Example
/// ```python
/// False == False in [False]  # True
/// ```
///
/// Use instead:
/// ```python
/// (False == False) in [False]  # False
/// ```
///
/// Or:
/// ```python
/// False == (False in [False])  # False
/// ```
///
/// Or, if the compound behavior is intended:
/// ```python
/// False == False and False in [False]  # True
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[violation]
pub struct NonTransitiveCompoundComparison;

impl Violation for NonTransitiveCompoundComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid non-transitive compound comparisons, which can be confusing")
    }
}

/// RUF029
pub(crate) fn non_transitive_compound_comparison(
    checker: &mut Checker,
    compare: &ast::ExprCompare,
) {
    // Make sure it's a compound comparison.
    if compare.ops.len() < 2 {
        return;
    }
    // A mix of `<` and `<=` is okay.
    if compare
        .ops
        .iter()
        .all(|op| *op == CmpOp::Lt || *op == CmpOp::LtE)
    {
        return;
    }
    // A mix of `>` and `>=` is okay.
    if compare
        .ops
        .iter()
        .all(|op| *op == CmpOp::Gt || *op == CmpOp::GtE)
    {
        return;
    }
    // All `is` is okay.
    if compare.ops.iter().all(|op| *op == CmpOp::Is) {
        return;
    }
    // All `==` is okay.
    if compare.ops.iter().all(|op| *op == CmpOp::Eq) {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        NonTransitiveCompoundComparison,
        compare.range(),
    ));
}
