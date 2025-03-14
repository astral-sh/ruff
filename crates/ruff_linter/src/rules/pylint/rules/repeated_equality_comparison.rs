use itertools::Itertools;
use rustc_hash::{FxBuildHasher, FxHashMap};

use ast::ExprContext;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::{any_over_expr, contains_effect};
use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::Locator;

/// ## What it does
/// Checks for repeated equality comparisons that can be rewritten as a membership
/// test.
///
/// This rule will try to determine if the values are hashable
/// and the fix will use a `set` if they are. If unable to determine, the fix
/// will use a `tuple` and suggest the use of a `set`.
///
/// ## Why is this bad?
/// To check if a variable is equal to one of many values, it is common to
/// write a series of equality comparisons (e.g.,
/// `foo == "bar" or foo == "baz"`).
///
/// Instead, prefer to combine the values into a collection and use the `in`
/// operator to check for membership, which is more performant and succinct.
/// If the items are hashable, use a `set` for efficiency; otherwise, use a
/// `tuple`.
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
#[derive(ViolationMetadata)]
pub(crate) struct RepeatedEqualityComparison {
    expression: SourceCodeSnippet,
    all_hashable: bool,
}

impl AlwaysFixableViolation for RepeatedEqualityComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        match (self.expression.full_display(), self.all_hashable) {
            (Some(expression), false) => {
                format!("Consider merging multiple comparisons: `{expression}`. Use a `set` if the elements are hashable.")
            }
            (Some(expression), true) => {
                format!("Consider merging multiple comparisons: `{expression}`.")
            }
            (None, false) => {
                "Consider merging multiple comparisons. Use a `set` if the elements are hashable."
                    .to_string()
            }
            (None, true) => "Consider merging multiple comparisons.".to_string(),
        }
    }

    fn fix_title(&self) -> String {
        "Merge multiple comparisons".to_string()
    }
}

/// PLR1714
pub(crate) fn repeated_equality_comparison(checker: &Checker, bool_op: &ast::ExprBoolOp) {
    // Map from expression hash to (starting offset, number of comparisons, list
    let mut value_to_comparators: FxHashMap<ComparableExpr, (&Expr, Vec<&Expr>, Vec<usize>)> =
        FxHashMap::with_capacity_and_hasher(bool_op.values.len() * 2, FxBuildHasher);

    for (i, value) in bool_op.values.iter().enumerate() {
        let Some((left, right)) = to_allowed_value(bool_op.op, value, checker.semantic()) else {
            continue;
        };

        if matches!(left, Expr::Name(_) | Expr::Attribute(_)) {
            let (_, left_matches, index_matches) = value_to_comparators
                .entry(left.into())
                .or_insert_with(|| (left, Vec::new(), Vec::new()));
            left_matches.push(right);
            index_matches.push(i);
        }

        if matches!(right, Expr::Name(_) | Expr::Attribute(_)) {
            let (_, right_matches, index_matches) = value_to_comparators
                .entry(right.into())
                .or_insert_with(|| (right, Vec::new(), Vec::new()));
            right_matches.push(left);
            index_matches.push(i);
        }
    }

    for (expr, comparators, indices) in value_to_comparators
        .into_values()
        .sorted_by_key(|(expr, _, _)| expr.start())
    {
        // If there's only one comparison, there's nothing to merge.
        if comparators.len() == 1 {
            continue;
        }

        // Break into sequences of consecutive comparisons.
        let mut sequences: Vec<(Vec<usize>, Vec<&Expr>)> = Vec::new();
        let mut last = None;
        for (index, comparator) in indices.iter().zip(comparators) {
            if last.is_some_and(|last| last + 1 == *index) {
                let (indices, comparators) = sequences.last_mut().unwrap();
                indices.push(*index);
                comparators.push(comparator);
            } else {
                sequences.push((vec![*index], vec![comparator]));
            }
            last = Some(*index);
        }

        for (indices, comparators) in sequences {
            if indices.len() == 1 {
                continue;
            }

            // if we can determine that all the values are hashable, we can use a set
            // TODO: improve with type inference
            let all_hashable = comparators
                .iter()
                .all(|comparator| comparator.is_literal_expr());

            let mut diagnostic = Diagnostic::new(
                RepeatedEqualityComparison {
                    expression: SourceCodeSnippet::new(merged_membership_test(
                        expr,
                        bool_op.op,
                        &comparators,
                        checker.locator(),
                        all_hashable,
                    )),
                    all_hashable,
                },
                bool_op.range(),
            );

            // Grab the remaining comparisons.
            let [first, .., last] = indices.as_slice() else {
                unreachable!("Indices should have at least two elements")
            };
            let before = bool_op.values.iter().take(*first).cloned();
            let after = bool_op.values.iter().skip(last + 1).cloned();

            let comparator = if all_hashable {
                Expr::Set(ast::ExprSet {
                    elts: comparators.iter().copied().cloned().collect(),
                    range: TextRange::default(),
                })
            } else {
                Expr::Tuple(ast::ExprTuple {
                    elts: comparators.iter().copied().cloned().collect(),
                    range: TextRange::default(),
                    ctx: ExprContext::Load,
                    parenthesized: true,
                })
            };

            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().expr(&Expr::BoolOp(ast::ExprBoolOp {
                    op: bool_op.op,
                    values: before
                        .chain(std::iter::once(Expr::Compare(ast::ExprCompare {
                            left: Box::new(expr.clone()),
                            ops: match bool_op.op {
                                BoolOp::Or => Box::from([CmpOp::In]),
                                BoolOp::And => Box::from([CmpOp::NotIn]),
                            },
                            comparators: Box::from([comparator]),
                            range: bool_op.range(),
                        })))
                        .chain(after)
                        .collect(),
                    range: bool_op.range(),
                })),
                bool_op.range(),
            )));

            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Return `true` if the given expression is compatible with a membership test.
/// E.g., `==` operators can be joined with `or` and `!=` operators can be
/// joined with `and`.
fn to_allowed_value<'a>(
    bool_op: BoolOp,
    value: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a Expr, &'a Expr)> {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = value
    else {
        return None;
    };

    // Ignore, e.g., `foo == bar == baz`.
    let [op] = &**ops else {
        return None;
    };

    if match bool_op {
        BoolOp::Or => !matches!(op, CmpOp::Eq),
        BoolOp::And => !matches!(op, CmpOp::NotEq),
    } {
        return None;
    }

    // Ignore self-comparisons, e.g., `foo == foo`.
    let [right] = &**comparators else {
        return None;
    };
    if ComparableExpr::from(left) == ComparableExpr::from(right) {
        return None;
    }

    if contains_effect(value, |id| semantic.has_builtin_binding(id)) {
        return None;
    }

    // Ignore `sys.version_info` and `sys.platform` comparisons, which are only
    // respected by type checkers when enforced via equality.
    if any_over_expr(value, &|expr| {
        semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["sys", "version_info" | "platform"]
                )
            })
    }) {
        return None;
    }

    Some((left, right))
}

/// Generate a string like `obj in (a, b, c)` or `obj not in (a, b, c)`.
/// If `all_hashable` is `true`, the string will use a set instead of a tuple.
fn merged_membership_test(
    left: &Expr,
    op: BoolOp,
    comparators: &[&Expr],
    locator: &Locator,
    all_hashable: bool,
) -> String {
    let op = match op {
        BoolOp::Or => "in",
        BoolOp::And => "not in",
    };
    let left = locator.slice(left);
    let members = comparators
        .iter()
        .map(|comparator| locator.slice(comparator))
        .join(", ");

    if all_hashable {
        return format!("{left} {op} {{{members}}}",);
    }

    format!("{left} {op} ({members})",)
}
