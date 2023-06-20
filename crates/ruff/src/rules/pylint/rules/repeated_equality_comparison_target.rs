use crate::checkers::ast::Checker;
use itertools::any;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::hashable::HashableExpr;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{BoolOp, CmpOp, Expr, ExprCompare, Ranged};
use std::ops::Deref;

/// ## What it does
/// Checks for equality comparisons that can rewritten as a membership test.
///
/// ## Why is this bad?
/// Instead, to check if a variable is equal to one of many values, combine the
/// values into a collection and use the `in` operator. This is faster and less
/// verbose. If the items are hashable, use a `set` instead of a `list`, as
/// membership tests are faster for sets.
///
/// ## Example
/// ```python
/// foo == "bar" or foo == "baz" or foo == "qux"
/// ```
///
/// Use instead:
/// ```python
/// foo in {"bar", "baz", "qux"}
/// ```
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
/// - [Python documentation: `set`](https://docs.python.org/3/library/stdtypes.html#set)
#[violation]
pub struct RepeatedEqualityComparisonTarget {
    membership_test: String,
}

impl Violation for RepeatedEqualityComparisonTarget {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RepeatedEqualityComparisonTarget { membership_test } = self;
        format!(
            "Consider merging multiple comparisons with `{membership_test}`. \
            Use a `set` if the elements are hashable."
        )
    }
}

fn is_allowed_value(bool_op: BoolOp, value: &Expr) -> bool {
    match value {
        Expr::Compare(ExprCompare {
            left,
            ops,
            comparators,
            ..
        }) => {
            for cmp_op in ops {
                if match bool_op {
                    BoolOp::Or => !matches!(cmp_op, CmpOp::Eq),
                    BoolOp::And => !matches!(cmp_op, CmpOp::NotEq),
                } {
                    return false;
                }
                if left.is_call_expr() {
                    return false;
                }
                if any(comparators.iter(), Expr::is_call_expr) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

/// PLR0124
pub(crate) fn repeated_equality_comparison_target(
    checker: &mut Checker,
    expr: &Expr,
    op: BoolOp,
    values: &[Expr],
) {
    if !op.is_or() && !op.is_and() {
        return;
    }
    if any(values.iter(), |v| !is_allowed_value(op, v)) {
        return;
    }
    let mut left_to_comparators: FxHashMap<HashableExpr, (usize, FxHashSet<HashableExpr>)> =
        FxHashMap::default();
    for value in values {
        match value {
            Expr::Compare(ExprCompare {
                left, comparators, ..
            }) => {
                let (count, comparators) = left_to_comparators
                    .entry(left.deref().into())
                    .or_insert_with(|| (0, FxHashSet::default()));
                *count += 1;
            }
            _ => continue,
        }
    }
    for (left, (count, comparators)) in left_to_comparators {
        if count < 2 {
            continue;
        }
        let msg = merged_membership_test(
            &checker.generator().expr(left.as_expr()),
            comparators
                .iter()
                .map(|c| checker.generator().expr(c.as_expr()))
                .collect::<Vec<String>>(),
            op,
        );

        checker.diagnostics.push(Diagnostic::new(
            RepeatedEqualityComparisonTarget {
                membership_test: msg,
            },
            expr.range(),
        ));
    }
}
fn merged_membership_test(
    obj: &str,
    collection_items: impl IntoIterator<Item = String>,
    op: BoolOp,
) -> String {
    let membership_op = match op {
        BoolOp::Or => "in",
        BoolOp::And => "not in",
    };
    format!(
        "{} {} ({})",
        obj,
        membership_op,
        collection_items
            .into_iter()
            .collect::<Vec<String>>()
            .join(", ")
    )
}
