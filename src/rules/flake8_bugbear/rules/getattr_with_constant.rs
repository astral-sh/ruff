use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location};

use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::python::identifiers::{is_identifier, is_mangled_private};
use crate::python::keyword::KWLIST;
use crate::registry::Diagnostic;
use crate::violations;

fn attribute(value: &Expr, attr: &str) -> Expr {
    Expr::new(
        Location::default(),
        Location::default(),
        ExprKind::Attribute {
            value: Box::new(value.clone()),
            attr: attr.to_string(),
            ctx: ExprContext::Load,
        },
    )
}

/// B009
pub fn getattr_with_constant(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    if id != "getattr" {
        return;
    }
    let [obj, arg] = args else {
        return;
    };
    let ExprKind::Constant {
        value: Constant::Str(value),
        ..
    } = &arg.node else {
        return;
    };
    if !is_identifier(value) {
        return;
    }
    if KWLIST.contains(&value.as_str()) || is_mangled_private(value.as_str()) {
        return;
    }

    let mut diagnostic =
        Diagnostic::new(violations::GetAttrWithConstant, Range::from_located(expr));

    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            unparse_expr(&attribute(obj, value), checker.stylist),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
