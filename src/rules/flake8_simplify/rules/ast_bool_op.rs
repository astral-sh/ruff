use std::collections::BTreeMap;
use std::iter;

use itertools::Either::{Left, Right};
use rustc_hash::FxHashMap;
use rustpython_ast::{Boolop, Cmpop, Constant, Expr, ExprContext, ExprKind, Unaryop};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;

/// Return `true` if two `Expr` instances are equivalent names.
fn is_same_expr<'a>(a: &'a Expr, b: &'a Expr) -> Option<&'a str> {
    if let (ExprKind::Name { id: a, .. }, ExprKind::Name { id: b, .. }) = (&a.node, &b.node) {
        if a == b {
            return Some(a);
        }
    }
    None
}

/// SIM101
pub fn duplicate_isinstance_call(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values } = &expr.node else {
        return;
    };

    // Locate duplicate `isinstance` calls, represented as a map from argument name
    // to indices of the relevant `Expr` instances in `values`.
    let mut duplicates = FxHashMap::default();
    for (index, call) in values.iter().enumerate() {
        // Verify that this is an `isinstance` call.
        let ExprKind::Call { func, args, keywords } = &call.node else {
            continue;
        };
        if args.len() != 2 {
            continue;
        }
        if !keywords.is_empty() {
            continue;
        }
        let ExprKind::Name { id: func_name, .. } = &func.node else {
            continue;
        };
        if func_name != "isinstance" {
            continue;
        }

        // Collect the name of the argument.
        let ExprKind::Name { id: arg_name, .. } = &args[0].node else {
            continue;
        };
        duplicates
            .entry(arg_name.as_str())
            .or_insert_with(Vec::new)
            .push(index);
    }

    // Generate a `Diagnostic` for each duplicate.
    for (arg_name, indices) in duplicates {
        if indices.len() > 1 {
            let mut diagnostic = Diagnostic::new(
                violations::DuplicateIsinstanceCall(arg_name.to_string()),
                Range::from_located(expr),
            );
            if checker.patch(&RuleCode::SIM101) {
                // Grab the types used in each duplicate `isinstance` call.
                let types: Vec<&Expr> = indices
                    .iter()
                    .map(|index| &values[*index])
                    .map(|expr| {
                        let ExprKind::Call { args, ..} = &expr.node else {
                            unreachable!("Indices should only contain `isinstance` calls")
                        };
                        args.get(1).expect("`isinstance` should have two arguments")
                    })
                    .collect();

                // Generate a single `isinstance` call.
                let call = create_expr(ExprKind::Call {
                    func: Box::new(create_expr(ExprKind::Name {
                        id: "isinstance".to_string(),
                        ctx: ExprContext::Load,
                    })),
                    args: vec![
                        create_expr(ExprKind::Name {
                            id: arg_name.to_string(),
                            ctx: ExprContext::Load,
                        }),
                        create_expr(ExprKind::Tuple {
                            // Flatten all the types used across the `isinstance` calls.
                            elts: types
                                .iter()
                                .flat_map(|value| {
                                    if let ExprKind::Tuple { elts, .. } = &value.node {
                                        Left(elts.iter())
                                    } else {
                                        Right(iter::once(*value))
                                    }
                                })
                                .map(Clone::clone)
                                .collect(),
                            ctx: ExprContext::Load,
                        }),
                    ],
                    keywords: vec![],
                });

                // Generate the combined `BoolOp`.
                let bool_op = create_expr(ExprKind::BoolOp {
                    op: Boolop::Or,
                    values: iter::once(call)
                        .chain(
                            values
                                .iter()
                                .enumerate()
                                .filter(|(index, _)| !indices.contains(index))
                                .map(|(_, elt)| elt.clone()),
                        )
                        .collect(),
                });

                // Populate the `Fix`. Replace the _entire_ `BoolOp`. Note that if we have
                // multiple duplicates, the fixes will conflict.
                diagnostic.amend(Fix::replacement(
                    unparse_expr(&bool_op, checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// SIM109
pub fn compare_with_tuple(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values } = &expr.node else {
        return;
    };

    let mut id_to_values = BTreeMap::<&str, Vec<&Expr>>::new();
    for value in values {
        let ExprKind::Compare { left, ops, comparators } = &value.node else {
            continue;
        };
        if ops.len() != 1 || comparators.len() != 1 {
            continue;
        }
        if !matches!(&ops[0], Cmpop::Eq) {
            continue;
        }
        let ExprKind::Name { id, .. } = &left.node else {
            continue;
        };
        let comparator = &comparators[0];
        if !matches!(&comparator.node, ExprKind::Name { .. }) {
            continue;
        }
        id_to_values.entry(id).or_default().push(comparator);
    }

    for (value, values) in id_to_values {
        if values.len() == 1 {
            continue;
        }
        let str_values = values
            .iter()
            .map(|value| unparse_expr(value, checker.stylist))
            .collect();
        let mut diagnostic = Diagnostic::new(
            violations::CompareWithTuple(
                value.to_string(),
                str_values,
                unparse_expr(expr, checker.stylist),
            ),
            Range::from_located(expr),
        );
        if checker.patch(&RuleCode::SIM109) {
            // Create a `x in (a, b)` compare expr.
            let in_expr = create_expr(ExprKind::Compare {
                left: Box::new(create_expr(ExprKind::Name {
                    id: value.to_string(),
                    ctx: ExprContext::Load,
                })),
                ops: vec![Cmpop::In],
                comparators: vec![create_expr(ExprKind::Tuple {
                    elts: values.into_iter().map(Clone::clone).collect(),
                    ctx: ExprContext::Load,
                })],
            });
            diagnostic.amend(Fix::replacement(
                unparse_expr(&in_expr, checker.stylist),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}

/// SIM220
pub fn a_and_not_a(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::And, values, } = &expr.node else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } = &expr.node
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    violations::AAndNotA(id.to_string()),
                    Range::from_located(expr),
                );
                if checker.patch(&RuleCode::SIM220) {
                    diagnostic.amend(Fix::replacement(
                        "False".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// SIM221
pub fn a_or_not_a(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values, } = &expr.node else {
        return;
    };
    if values.len() < 2 {
        return;
    }

    // Collect all negated and non-negated expressions.
    let mut negated_expr = vec![];
    let mut non_negated_expr = vec![];
    for expr in values {
        if let ExprKind::UnaryOp {
            op: Unaryop::Not,
            operand,
        } = &expr.node
        {
            negated_expr.push(operand);
        } else {
            non_negated_expr.push(expr);
        }
    }

    if negated_expr.is_empty() {
        return;
    }

    for negate_expr in negated_expr {
        for non_negate_expr in &non_negated_expr {
            if let Some(id) = is_same_expr(negate_expr, non_negate_expr) {
                let mut diagnostic = Diagnostic::new(
                    violations::AOrNotA(id.to_string()),
                    Range::from_located(expr),
                );
                if checker.patch(&RuleCode::SIM220) {
                    diagnostic.amend(Fix::replacement(
                        "True".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// SIM222
pub fn or_true(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::Or, values, } = &expr.node else {
        return;
    };
    for value in values {
        if let ExprKind::Constant {
            value: Constant::Bool(true),
            ..
        } = &value.node
        {
            let mut diagnostic = Diagnostic::new(violations::OrTrue, Range::from_located(value));
            if checker.patch(&RuleCode::SIM223) {
                diagnostic.amend(Fix::replacement(
                    "True".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// SIM223
pub fn and_false(checker: &mut Checker, expr: &Expr) {
    let ExprKind::BoolOp { op: Boolop::And, values, } = &expr.node else {
        return;
    };
    for value in values {
        if let ExprKind::Constant {
            value: Constant::Bool(false),
            ..
        } = &value.node
        {
            let mut diagnostic = Diagnostic::new(violations::AndFalse, Range::from_located(value));
            if checker.patch(&RuleCode::SIM223) {
                diagnostic.amend(Fix::replacement(
                    "False".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
