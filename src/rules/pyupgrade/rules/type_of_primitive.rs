use rustpython_ast::{Expr, ExprKind};

use super::super::types::Primitive;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

/// UP003
pub fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if args.len() != 1 {
        return;
    }
    if !checker
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["", "type"])
    {
        return;
    }
    let ExprKind::Constant { value, .. } = &args[0].node else {
        return;
    };
    let Some(primitive) = Primitive::from_constant(value) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        violations::TypeOfPrimitive(primitive),
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            primitive.builtin(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
