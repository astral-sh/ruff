use rustpython_ast::{Boolop, Constant, Expr, ExprKind, Unaryop};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};

fn is_same_expr<'a>(a: &'a Expr, b: &'a Expr) -> Option<&'a str> {
    if let (ExprKind::Name { id: a, .. }, ExprKind::Name { id: b, .. }) = (&a.node, &b.node) {
        if a == b {
            return Some(a);
        }
    }
    None
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
                let mut check = Check::new(
                    CheckKind::AAndNotA(id.to_string()),
                    Range::from_located(expr),
                );
                if checker.patch(&CheckCode::SIM220) {
                    check.amend(Fix::replacement(
                        "False".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.add_check(check);
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
                let mut check = Check::new(
                    CheckKind::AOrNotA(id.to_string()),
                    Range::from_located(expr),
                );
                if checker.patch(&CheckCode::SIM220) {
                    check.amend(Fix::replacement(
                        "True".to_string(),
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                checker.add_check(check);
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
            let mut check = Check::new(CheckKind::OrTrue, Range::from_located(value));
            if checker.patch(&CheckCode::SIM223) {
                check.amend(Fix::replacement(
                    "True".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
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
            let mut check = Check::new(CheckKind::AndFalse, Range::from_located(value));
            if checker.patch(&CheckCode::SIM223) {
                check.amend(Fix::replacement(
                    "False".to_string(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}
