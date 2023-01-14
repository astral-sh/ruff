use rustpython_ast::{Expr, ExprKind};

use super::super::types::Primitive;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, DiagnosticKind};
use crate::violations;

fn rule(func: &Expr, args: &[Expr], location: Range) -> Option<Diagnostic> {
    // Validate the arguments.
    if args.len() != 1 {
        return None;
    }

    let (ExprKind::Attribute { attr: id, .. } | ExprKind::Name { id, .. }) = &func.node else {
        return None;
    };
    if id != "type" {
        return None;
    }

    let ExprKind::Constant { value, .. } = &args[0].node else {
        return None;
    };

    let primitive = Primitive::from_constant(value)?;
    Some(Diagnostic::new(
        violations::TypeOfPrimitive(primitive),
        location,
    ))
}

/// UP003
pub fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let Some(mut diagnostic) = rule(func, args, Range::from_located(expr)) else {
        return;
    };
    if checker.patch(diagnostic.kind.code()) {
        if let DiagnosticKind::TypeOfPrimitive(violations::TypeOfPrimitive(primitive)) =
            &diagnostic.kind
        {
            diagnostic.amend(Fix::replacement(
                primitive.builtin(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    checker.diagnostics.push(diagnostic);
}
