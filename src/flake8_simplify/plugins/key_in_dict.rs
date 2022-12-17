use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};

/// SIM118
fn key_in_dict(checker: &mut Checker, left: &Expr, right: &Expr, range: Range) {
    let ExprKind::Call {
        func,
        args,
        keywords,
    } = &right.node else {
        return;
    };
    if !(args.is_empty() && keywords.is_empty()) {
        return;
    }

    let ExprKind::Attribute { attr, value, .. } = &func.node else {
        return;
    };
    if attr != "keys" {
        return;
    }

    let mut check = Check::new(
        CheckKind::KeyInDict(left.to_string(), value.to_string()),
        range,
    );
    if checker.patch(&CheckCode::SIM118) {
        let content = right.to_string().replace(".keys()", "");
        check.amend(Fix::replacement(
            content,
            right.location,
            right.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}

/// SIM118 in a for loop
pub fn key_in_dict_for(checker: &mut Checker, target: &Expr, iter: &Expr) {
    key_in_dict(
        checker,
        target,
        iter,
        Range {
            location: target.location,
            end_location: iter.end_location.unwrap(),
        },
    );
}

/// SIM118 in a comparison
pub fn key_in_dict_compare(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    if !matches!(ops[..], [Cmpop::In]) {
        return;
    }

    if comparators.len() != 1 {
        return;
    }
    let right = comparators.first().unwrap();

    key_in_dict(checker, left, right, Range::from_located(expr));
}
