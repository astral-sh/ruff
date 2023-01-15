use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;
use crate::registry::Diagnostic;
use crate::source_code::Generator;
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
    if !IDENTIFIER_REGEX.is_match(value) {
        return;
    }
    if KWLIST.contains(&value.as_str()) {
        return;
    }

    let mut diagnostic =
        Diagnostic::new(violations::GetAttrWithConstant, Range::from_located(expr));
    if checker.patch(diagnostic.kind.code()) {
        let mut generator: Generator = checker.stylist.into();
        generator.unparse_expr(&attribute(obj, value), 0);
        diagnostic.amend(Fix::replacement(
            generator.generate(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
