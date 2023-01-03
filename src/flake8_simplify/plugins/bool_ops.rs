use rustpython_ast::{Boolop, Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};

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
