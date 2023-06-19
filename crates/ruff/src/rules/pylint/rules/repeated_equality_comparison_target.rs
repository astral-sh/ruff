use rustpython_parser::ast::{Boolop, Expr, ExprBoolOp, ExprCompare};

use ruff_diagnostics::{Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for operations that compare a name to itself.
///
/// ## Why is this bad?
/// Comparing a name to itself always results in the same value, and is likely
/// a mistake.
///
/// ## Example
/// ```python
/// foo == foo
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
#[violation]
pub struct RepeatedEqualityComparisonTarget;

impl Violation for RepeatedEqualityComparisonTarget {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Consider merging multiple comparisons with ???. \
            Use a `set` if the elements are hashable."
        )
    }
}


/// PLR0124
pub(crate) fn repeated_equality_comparison_target(
    _checker: &mut Checker,
    op: Boolop,
    values: &[Expr],
) {
    // Ignore if the operators are not `or`.
    if op != Boolop::Or {
        return;
    }
    for value in values {
         if let Expr::BoolOp(ExprBoolOp { op, .. }) = value {
             if *op != Boolop::Or {
                 return;
             }
         }
    }

    println!("Found an expression linked by `or`");

    // Iterate though `values` ExprCompare type elements.
    // If the left value is the same in every element of `values`, then
    // we have a repeated comparison target.
    let mut prev_names = Vec::new();
    let mut prev_consts = Vec::new();
    // For each item in `values`, check if prev_names or prev_const exist in `left` or `comparator`
    // elements. If there is any element in prev_names or prev_consts that does not exst in the
    // current value being checked, remove it from prev_names or prev_consts. If the last element
    // in prev_names or prev_consts is removed, return. If prev_names or prev_consts is empty,
    // that means it is the start of the loop, so add all the elements in `left` and `comparator`
    // to prev_names and prev_consts.
    for value in values {
        let current_left = match value {
            Expr::Compare(ExprCompare { left, .. }) => left,
            _ => return,
        };
        let current_comparators = match value {
            Expr::Compare(ExprCompare { comparators, .. }) => comparators,
            _ => return,
        };
        // If prev_names and prev_consts are empty, add all the elements in `left` and `comparator`
        // to prev_names and prev_consts. Then continue to the next value in `values`.
        if prev_names.is_empty() && prev_consts.is_empty() {
            if let Expr::Name(name) = current_left {
                prev_names.push(name);
            } else if let Expr::Constant(constant) = current_left {
                prev_consts.push(constant);
            }
            for comparator in current_comparators {
                if let Expr::Name(name) = comparator {
                    prev_names.push(name);
                } else if let Expr::Constant(constant) = comparator {
                    prev_consts.push(constant);
                }
            }
            // Print the contents of prev_names and prev_consts for debugging.
            println!("prev_names: {:?}", prev_names);
            println!("prev_consts: {:?}", prev_consts);
            continue;
        }
    }

    println!("Found a repeated comparison target");

}
