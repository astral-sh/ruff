use itertools::Itertools;
use ruff_python_ast::{self as ast, BoolOp, Expr};
use ruff_text_size::{Ranged, TextRange};

use ruff_python_ast::helpers::{Truthiness, contains_effect};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_codegen::Generator;

use crate::checkers::ast::Checker;
use crate::Edit;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ContentAround {
    Before,
    After,
    Both,
}


/// Return `true` if two `Expr` instances are equivalent names.
pub(super) fn is_same_expr<'a>(a: &'a Expr, b: &'a Expr) -> Option<&'a str> {
    if let (Expr::Name(ast::ExprName { id: a, .. }), Expr::Name(ast::ExprName { id: b, .. })) =
        (&a, &b)
    {
        if a == b {
            return Some(a);
        }
    }
    None
}


fn get_short_circuit_edit(
    expr: &Expr,
    range: TextRange,
    truthiness: bool,
    in_boolean_test: bool,
    generator: Generator,
) -> Edit {
    let content = if in_boolean_test {
        if truthiness {
            "True".to_string()
        } else {
            "False".to_string()
        }
    } else {
        generator.expr(expr)
    };
    Edit::range_replacement(
        if matches!(expr, Expr::Tuple(tuple) if !tuple.is_empty()) {
            format!("({content})")
        } else {
            content
        },
        range,
    )
}

pub(super) fn is_short_circuit(
    expr: &Expr,
    expected_op: BoolOp,
    checker: &Checker,
) -> Option<(Edit, ContentAround)> {
    let Expr::BoolOp(ast::ExprBoolOp {
        op,
        values,
        range: _,
        node_index: _,
    }) = expr
    else {
        return None;
    };
    if *op != expected_op {
        return None;
    }
    let short_circuit_truthiness = match op {
        BoolOp::And => false,
        BoolOp::Or => true,
    };

    let mut furthest = expr;
    let mut edit = None;
    let mut remove = None;

    for (index, (value, next_value)) in values.iter().tuple_windows().enumerate() {
        // Keep track of the location of the furthest-right, truthy or falsey expression.
        let value_truthiness =
            Truthiness::from_expr(value, |id| checker.semantic().has_builtin_binding(id));
        let next_value_truthiness =
            Truthiness::from_expr(next_value, |id| checker.semantic().has_builtin_binding(id));

        // Keep track of the location of the furthest-right, non-effectful expression.
        if value_truthiness.is_unknown()
            && (!checker.semantic().in_boolean_test()
                || contains_effect(value, |id| checker.semantic().has_builtin_binding(id)))
        {
            furthest = next_value;
            continue;
        }

        // If the current expression is a constant, and it matches the short-circuit value, then
        // we can return the location of the expression. This should only trigger if the
        // short-circuit expression is the first expression in the list; otherwise, we'll see it
        // as `next_value` before we see it as `value`.
        if value_truthiness.into_bool() == Some(short_circuit_truthiness) {
            remove = Some(ContentAround::After);

            edit = Some(get_short_circuit_edit(
                value,
                TextRange::new(
                    parenthesized_range(
                        furthest.into(),
                        expr.into(),
                        checker.comment_ranges(),
                        checker.locator().contents(),
                    )
                    .unwrap_or(furthest.range())
                    .start(),
                    expr.end(),
                ),
                short_circuit_truthiness,
                checker.semantic().in_boolean_test(),
                checker.generator(),
            ));
            break;
        }

        // If the next expression is a constant, and it matches the short-circuit value, then
        // we can return the location of the expression.
        if next_value_truthiness.into_bool() == Some(short_circuit_truthiness) {
            remove = Some(if index + 1 == values.len() - 1 {
                ContentAround::Before
            } else {
                ContentAround::Both
            });
            edit = Some(get_short_circuit_edit(
                next_value,
                TextRange::new(
                    parenthesized_range(
                        furthest.into(),
                        expr.into(),
                        checker.comment_ranges(),
                        checker.locator().contents(),
                    )
                    .unwrap_or(furthest.range())
                    .start(),
                    expr.end(),
                ),
                short_circuit_truthiness,
                checker.semantic().in_boolean_test(),
                checker.generator(),
            ));
            break;
        }
    }

    match (edit, remove) {
        (Some(edit), Some(remove)) => Some((edit, remove)),
        _ => None,
    }
}