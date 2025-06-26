use std::collections::BTreeMap;
use std::iter;

use ruff_python_ast::{self as ast, BoolOp, CmpOp, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::name::Name;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};
/// ## What it does
/// Checks for boolean expressions that contain multiple equality comparisons
/// to the same value.
///
/// ## Why is this bad?
/// To check if an object is equal to any one of multiple values, it's more
/// concise to use the `in` operator with a tuple of values.
///
/// ## Example
/// ```python
/// if foo == x or foo == y:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if foo in (x, y):
///     ...
/// ```
///
/// ## References
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
#[derive(ViolationMetadata)]
pub(crate) struct CompareWithTuple {
    replacement: String,
}

impl AlwaysFixableViolation for CompareWithTuple {
    #[derive_message_formats]
    fn message(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Use `{replacement}` instead of multiple equality comparisons")
    }

    fn fix_title(&self) -> String {
        let CompareWithTuple { replacement } = self;
        format!("Replace with `{replacement}`")
    }
}


fn match_eq_target(expr: &Expr) -> Option<(&Name, &Expr)> {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        range: _,
        node_index: _,
    }) = expr
    else {
        return None;
    };
    if **ops != [CmpOp::Eq] {
        return None;
    }
    let Expr::Name(ast::ExprName { id, .. }) = &**left else {
        return None;
    };
    let [comparator] = &**comparators else {
        return None;
    };
    if !comparator.is_name_expr() {
        return None;
    }
    Some((id, comparator))
}

/// SIM109
pub(crate) fn compare_with_tuple(checker: &Checker, expr: &Expr) {
    let Expr::BoolOp(ast::ExprBoolOp {
        op: BoolOp::Or,
        values,
        range: _,
        node_index: _,
    }) = expr
    else {
        return;
    };

    // Given `a == "foo" or a == "bar"`, we generate `{"a": [(0, "foo"), (1,
    // "bar")]}`.
    let mut id_to_comparators: BTreeMap<&Name, Vec<(usize, &Expr)>> = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        if let Some((id, comparator)) = match_eq_target(value) {
            id_to_comparators
                .entry(id)
                .or_default()
                .push((index, comparator));
        }
    }

    for (id, matches) in id_to_comparators {
        if matches.len() == 1 {
            continue;
        }

        let (indices, comparators): (Vec<_>, Vec<_>) = matches.iter().copied().unzip();

        // Avoid rewriting (e.g.) `a == "foo" or a == f()`.
        if comparators
            .iter()
            .any(|expr| contains_effect(expr, |id| checker.semantic().has_builtin_binding(id)))
        {
            continue;
        }

        // Avoid removing comments.
        if checker
            .comment_ranges()
            .has_comments(expr, checker.source())
        {
            continue;
        }

        // Create a `x in (a, b)` expression.
        let node = ast::ExprTuple {
            elts: comparators.into_iter().cloned().collect(),
            ctx: ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
            parenthesized: true,
        };
        let node1 = ast::ExprName {
            id: id.clone(),
            ctx: ExprContext::Load,
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
        };
        let node2 = ast::ExprCompare {
            left: Box::new(node1.into()),
            ops: Box::from([CmpOp::In]),
            comparators: Box::from([node.into()]),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
        };
        let in_expr = node2.into();
        let mut diagnostic = checker.report_diagnostic(
            CompareWithTuple {
                replacement: checker.generator().expr(&in_expr),
            },
            expr.range(),
        );
        let unmatched: Vec<Expr> = values
            .iter()
            .enumerate()
            .filter(|(index, _)| !indices.contains(index))
            .map(|(_, elt)| elt.clone())
            .collect();
        let in_expr = if unmatched.is_empty() {
            in_expr
        } else {
            // Wrap in a `x in (a, b) or ...` boolean operation.
            let node = ast::ExprBoolOp {
                op: BoolOp::Or,
                values: iter::once(in_expr).chain(unmatched).collect(),
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
            };
            node.into()
        };
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(&in_expr),
            expr.range(),
        )));
    }
}
