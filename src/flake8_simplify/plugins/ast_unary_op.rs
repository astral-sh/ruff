use rustpython_ast::{Cmpop, Expr, ExprKind, Stmt, StmtKind, Unaryop};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

fn is_exception_check(stmt: &Stmt) -> bool {
    let StmtKind::If {test: _, body, orelse: _} = &stmt.node else {
        return false;
    };
    if body.len() != 1 {
        return false;
    }
    if matches!(body[0].node, StmtKind::Raise { .. }) {
        return true;
    }
    false
}

/// SIM201
pub fn negation_with_equal_op(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let ExprKind::Compare{ left, ops, comparators} = &operand.node else {
        return;
    };
    if !matches!(&ops[..], [Cmpop::Eq]) {
        return;
    }
    if is_exception_check(xxxxxxxx.current_stmt()) {
        return;
    }

    let mut check = Diagnostic::new(
        violations::NegateEqualOp(
            unparse_expr(left, xxxxxxxx.style),
            unparse_expr(&comparators[0], xxxxxxxx.style),
        ),
        Range::from_located(operand),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::Compare {
                    left: left.clone(),
                    ops: vec![Cmpop::NotEq],
                    comparators: comparators.clone(),
                }),
                xxxxxxxx.style,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

/// SIM202
pub fn negation_with_not_equal_op(
    xxxxxxxx: &mut xxxxxxxx,
    expr: &Expr,
    op: &Unaryop,
    operand: &Expr,
) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let ExprKind::Compare{ left, ops, comparators} = &operand.node else {
        return;
    };
    if !matches!(&ops[..], [Cmpop::NotEq]) {
        return;
    }
    if is_exception_check(xxxxxxxx.current_stmt()) {
        return;
    }

    let mut check = Diagnostic::new(
        violations::NegateNotEqualOp(
            unparse_expr(left, xxxxxxxx.style),
            unparse_expr(&comparators[0], xxxxxxxx.style),
        ),
        Range::from_located(operand),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::Compare {
                    left: left.clone(),
                    ops: vec![Cmpop::Eq],
                    comparators: comparators.clone(),
                }),
                xxxxxxxx.style,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

/// SIM208
pub fn double_negation(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, op: &Unaryop, operand: &Expr) {
    if !matches!(op, Unaryop::Not) {
        return;
    }
    let ExprKind::UnaryOp { op: operand_op, operand } = &operand.node else {
        return;
    };
    if !matches!(operand_op, Unaryop::Not) {
        return;
    }

    let mut check = Diagnostic::new(
        violations::DoubleNegation(operand.to_string()),
        Range::from_located(operand),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            unparse_expr(operand, xxxxxxxx.style),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}
