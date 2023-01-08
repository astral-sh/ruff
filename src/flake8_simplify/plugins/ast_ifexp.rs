use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Unaryop};

use crate::ast::helpers::{create_expr, unparse_expr};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// SIM210
pub fn explicit_true_false_in_ifexpr(
    xxxxxxxx: &mut xxxxxxxx,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let ExprKind::Constant { value, .. } = &body.node else {
        return;
    };
    if !matches!(value, Constant::Bool(true)) {
        return;
    }
    let ExprKind::Constant { value, .. } = &orelse.node else {
        return;
    };
    if !matches!(value, Constant::Bool(false)) {
        return;
    }

    let mut check = Diagnostic::new(
        violations::IfExprWithTrueFalse(unparse_expr(test, xxxxxxxx.style)),
        Range::from_located(expr),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::Call {
                    func: Box::new(create_expr(ExprKind::Name {
                        id: "bool".to_string(),
                        ctx: ExprContext::Load,
                    })),
                    args: vec![create_expr(test.node.clone())],
                    keywords: vec![],
                }),
                xxxxxxxx.style,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

/// SIM211
pub fn explicit_false_true_in_ifexpr(
    xxxxxxxx: &mut xxxxxxxx,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let ExprKind::Constant { value, .. } = &body.node else {
        return;
    };
    if !matches!(value, Constant::Bool(false)) {
        return;
    }
    let ExprKind::Constant { value, .. } = &orelse.node else {
        return;
    };
    if !matches!(value, Constant::Bool(true)) {
        return;
    }

    let mut check = Diagnostic::new(
        violations::IfExprWithFalseTrue(unparse_expr(test, xxxxxxxx.style)),
        Range::from_located(expr),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::UnaryOp {
                    op: Unaryop::Not,
                    operand: Box::new(create_expr(test.node.clone())),
                }),
                xxxxxxxx.style,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}

/// SIM212
pub fn twisted_arms_in_ifexpr(
    xxxxxxxx: &mut xxxxxxxx,
    expr: &Expr,
    test: &Expr,
    body: &Expr,
    orelse: &Expr,
) {
    let ExprKind::UnaryOp { op, operand: test_operand } = &test.node else {
        return;
    };
    if !matches!(op, Unaryop::Not) {
        return;
    }

    // Check if the test operand and else branch use the same variable.
    let ExprKind::Name { id: test_id, .. } = &test_operand.node else {
        return;
    };
    let ExprKind::Name {id: orelse_id, ..} = &orelse.node else {
        return;
    };
    if !test_id.eq(orelse_id) {
        return;
    }

    let mut check = Diagnostic::new(
        violations::IfExprWithTwistedArms(
            unparse_expr(body, xxxxxxxx.style),
            unparse_expr(orelse, xxxxxxxx.style),
        ),
        Range::from_located(expr),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            unparse_expr(
                &create_expr(ExprKind::IfExp {
                    test: Box::new(create_expr(orelse.node.clone())),
                    body: Box::new(create_expr(orelse.node.clone())),
                    orelse: Box::new(create_expr(body.node.clone())),
                }),
                xxxxxxxx.style,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}
