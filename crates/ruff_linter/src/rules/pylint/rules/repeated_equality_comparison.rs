use itertools::Itertools;
use rustc_hash::{FxBuildHasher, FxHashMap};

use ast::ExprContext;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::hashable::HashableExpr;
use ruff_python_ast::helpers::{any_over_expr, contains_effect};
use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr};
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for repeated equality comparisons that can rewritten as a membership
/// test.
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
#[violation]
pub struct RepeatedEqualityComparison {
    expression: SourceCodeSnippet,
}

impl AlwaysFixableViolation for RepeatedEqualityComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RepeatedEqualityComparison { expression } = self;
        if let Some(expression) = expression.full_display() {
            format!(
                "Consider merging multiple comparisons: `{expression}`. Use a `set` if the elements are hashable."
            )
        } else {
            format!(
                "Consider merging multiple comparisons. Use a `set` if the elements are hashable."
            )
        }
    }

    fn fix_title(&self) -> String {
        format!("Merge multiple comparisons")
    }
}

/// PLR1714
pub(crate) fn repeated_equality_comparison(checker: &mut Checker, bool_op: &ast::ExprBoolOp) {
    // Map from expression hash to (starting offset, number of comparisons, list
    let mut value_to_comparators: FxHashMap<HashableExpr, (TextSize, Vec<&Expr>, Vec<usize>)> =
        FxHashMap::with_capacity_and_hasher(bool_op.values.len() * 2, FxBuildHasher);

    for (i, value) in bool_op.values.iter().enumerate() {
        let Some((left, right)) = to_allowed_value(bool_op.op, value, checker.semantic()) else {
            continue;
        };

        if matches!(left, Expr::Name(_) | Expr::Attribute(_)) {
            let (_, left_matches, index_matches) = value_to_comparators
                .entry(left.into())
                .or_insert_with(|| (left.start(), Vec::new(), Vec::new()));
            left_matches.push(right);
            index_matches.push(i);
        }

        if matches!(right, Expr::Name(_) | Expr::Attribute(_)) {
            let (_, right_matches, index_matches) = value_to_comparators
                .entry(right.into())
                .or_insert_with(|| (right.start(), Vec::new(), Vec::new()));
            right_matches.push(left);
            index_matches.push(i);
        }
    }

    for (value, (_, comparators, indices)) in value_to_comparators
        .iter()
        .sorted_by_key(|(_, (start, _, _))| *start)
    {
        // If there's only one comparison, there's nothing to merge.
        if comparators.len() == 1 {
            continue;
        }

        // Break into sequences of consecutive comparisons.
        let mut sequences: Vec<(Vec<usize>, Vec<&Expr>)> = Vec::new();
        let mut last = None;
        for (index, comparator) in indices.iter().zip(comparators.iter()) {
            if last.is_some_and(|last| last + 1 == *index) {
                let (indices, comparators) = sequences.last_mut().unwrap();
                indices.push(*index);
                comparators.push(*comparator);
            } else {
                sequences.push((vec![*index], vec![*comparator]));
            }
            last = Some(*index);
        }

        for (indices, comparators) in sequences {
            if indices.len() == 1 {
                continue;
            }

            let mut diagnostic = Diagnostic::new(
                RepeatedEqualityComparison {
                    expression: SourceCodeSnippet::new(merged_membership_test(
                        value.as_expr(),
                        bool_op.op,
                        &comparators,
                        checker.locator(),
                    )),
                },
                bool_op.range(),
            );

            // Grab the remaining comparisons.
            let [first, .., last] = indices.as_slice() else {
                unreachable!("Indices should have at least two elements")
            };
            let before = bool_op.values.iter().take(*first).cloned();
            let after = bool_op.values.iter().skip(last + 1).cloned();

            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                checker.generator().expr(&Expr::BoolOp(ast::ExprBoolOp {
                    op: bool_op.op,
                    values: before
                        .chain(std::iter::once(Expr::Compare(ast::ExprCompare {
                            left: Box::new(value.as_expr().clone()),
                            ops: match bool_op.op {
                                BoolOp::Or => Box::from([CmpOp::In]),
                                BoolOp::And => Box::from([CmpOp::NotIn]),
                            },
                            comparators: Box::from([Expr::Tuple(ast::ExprTuple {
                                elts: comparators.iter().copied().cloned().collect(),
                                range: TextRange::default(),
                                ctx: ExprContext::Load,
                                parenthesized: true,
                            })]),
                            range: bool_op.range(),
                        })))
                        .chain(after)
                        .collect(),
                    range: bool_op.range(),
                })),
                bool_op.range(),
            )));

            checker.diagnostics.push(diagnostic);
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
fn merged_membership_test(
    left: &Expr,
    op: BoolOp,
    comparators: &[&Expr],
    locator: &Locator,
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
    format!("{left} {op} ({members})",)
}
