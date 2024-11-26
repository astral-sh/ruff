use std::iter::zip;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprCompare;
use ruff_python_ast::{CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `in` or `not in` use on single item container
///
/// ## Why is this bad?
/// `in` comparison with container containing only one item
/// looks like an overhead and unneeded complexity.
///
/// Consider using equality test instead.
///
/// ## Example
/// ```python
/// a in {"yes"}
/// ```
///
/// Use instead:
/// ```python
/// a == "yes"
/// ```
///
/// ## References
///  - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
#[violation]
pub struct InCompareWithSingleItemContainer;

impl Violation for InCompareWithSingleItemContainer {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid comparing to a single item container with membership operator".to_string()
    }
}

/// RUF051
pub(crate) fn in_compare_with_single_item_container(checker: &mut Checker, compare: &ExprCompare) {
    let ExprCompare {
        ops, comparators, ..
    } = compare;
    let diagnostics: Vec<Diagnostic> = zip(ops, comparators)
        .filter(|(op, comparator)| {
            matches!(op, CmpOp::In | CmpOp::NotIn) && is_single_item_container(comparator)
        })
        .map(|(_, comparator)| {
            Diagnostic::new(InCompareWithSingleItemContainer, comparator.range())
        })
        .collect();

    // Extend the checker diagnostics with the new diagnostics
    checker.diagnostics.extend(diagnostics);
}

fn is_single_item_container(expr: &Expr) -> bool {
    match expr {
        Expr::Dict(container) => container.len() == 1,
        Expr::Set(container) => {
            if container.len() != 1 {
                return false;
            }
            !container.elts[0].is_starred_expr()
        }
        Expr::List(container) => {
            if container.len() != 1 {
                return false;
            }
            !container.elts[0].is_starred_expr()
        }
        Expr::Tuple(container) => container.len() == 1,
        _ => false,
    }
}
