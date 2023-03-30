use rustpython_parser::ast::{Constant, Expr, ExprContext, ExprKind, Unaryop};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct IfExprWithTrueFalse {
    pub expr: String,
}

impl AlwaysAutofixableViolation for IfExprWithTrueFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTrueFalse { expr } = self;
        format!("Use `bool({expr})` instead of `True if {expr} else False`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTrueFalse { expr } = self;
        format!("Replace with `not {expr}")
    }
}

#[violation]
pub struct IfExprWithFalseTrue {
    pub expr: String,
}

impl AlwaysAutofixableViolation for IfExprWithFalseTrue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithFalseTrue { expr } = self;
        format!("Use `not {expr}` instead of `False if {expr} else True`")
    }

    fn autofix_title(&self) -> String {
        let IfExprWithFalseTrue { expr } = self;
        format!("Replace with `bool({expr})")
    }
}

#[violation]
pub struct IfExprWithTwistedArms {
    pub expr_body: String,
    pub expr_else: String,
}

impl AlwaysAutofixableViolation for IfExprWithTwistedArms {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!(
            "Use `{expr_else} if {expr_else} else {expr_body}` instead of `{expr_body} if not \
             {expr_else} else {expr_else}`"
        )
    }

    fn autofix_title(&self) -> String {
        let IfExprWithTwistedArms {
            expr_body,
            expr_else,
        } = self;
        format!("Replace with `{expr_else} if {expr_else} else {expr_body}`")
    }
}

/// SIM210
pub fn explicit_true_false_in_ifexpr(
    checker: &mut Checker,
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

    let mut diagnostic = Diagnostic::new(
        IfExprWithTrueFalse {
            expr: unparse_expr(test, checker.stylist),
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if matches!(test.node, ExprKind::Compare { .. }) {
            diagnostic.set_fix(Edit::replacement(
                unparse_expr(&test.clone(), checker.stylist),
                expr.location,
                expr.end_location.unwrap(),
            ));
        } else if checker.ctx.is_builtin("bool") {
            diagnostic.set_fix(Edit::replacement(
                unparse_expr(
                    &create_expr(ExprKind::Call {
                        func: Box::new(create_expr(ExprKind::Name {
                            id: "bool".to_string(),
                            ctx: ExprContext::Load,
                        })),
                        args: vec![test.clone()],
                        keywords: vec![],
                    }),
                    checker.stylist,
                ),
                expr.location,
                expr.end_location.unwrap(),
            ));
        };
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM211
pub fn explicit_false_true_in_ifexpr(
    checker: &mut Checker,
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

    let mut diagnostic = Diagnostic::new(
        IfExprWithFalseTrue {
            expr: unparse_expr(test, checker.stylist),
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            unparse_expr(
                &create_expr(ExprKind::UnaryOp {
                    op: Unaryop::Not,
                    operand: Box::new(create_expr(test.node.clone())),
                }),
                checker.stylist,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM212
pub fn twisted_arms_in_ifexpr(
    checker: &mut Checker,
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

    let mut diagnostic = Diagnostic::new(
        IfExprWithTwistedArms {
            expr_body: unparse_expr(body, checker.stylist),
            expr_else: unparse_expr(orelse, checker.stylist),
        },
        Range::from(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::replacement(
            unparse_expr(
                &create_expr(ExprKind::IfExp {
                    test: Box::new(create_expr(orelse.node.clone())),
                    body: Box::new(create_expr(orelse.node.clone())),
                    orelse: Box::new(create_expr(body.node.clone())),
                }),
                checker.stylist,
            ),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
