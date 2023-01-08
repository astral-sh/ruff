use rustpython_ast::{Cmpop, Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// SIM118
fn key_in_dict(xxxxxxxx: &mut xxxxxxxx, left: &Expr, right: &Expr, range: Range) {
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

    // Slice exact content to preserve formatting.
    let left_content = xxxxxxxx
        .locator
        .slice_source_code_range(&Range::from_located(left));
    let value_content = xxxxxxxx
        .locator
        .slice_source_code_range(&Range::from_located(value));

    let mut check = Diagnostic::new(
        violations::KeyInDict(left_content.to_string(), value_content.to_string()),
        range,
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            value_content.to_string(),
            right.location,
            right.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

/// SIM118 in a for loop
pub fn key_in_dict_for(xxxxxxxx: &mut xxxxxxxx, target: &Expr, iter: &Expr) {
    key_in_dict(
        xxxxxxxx,
        target,
        iter,
        Range::new(target.location, iter.end_location.unwrap()),
    );
}

/// SIM118 in a comparison
pub fn key_in_dict_compare(
    xxxxxxxx: &mut xxxxxxxx,
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

    key_in_dict(xxxxxxxx, left, right, Range::from_located(expr));
}
