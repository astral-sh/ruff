use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::pyupgrade::checks;
use crate::registry::DiagnosticKind;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP003
pub fn type_of_primitive(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, func: &Expr, args: &[Expr]) {
    let Some(mut check) = checks::type_of_primitive(func, args, Range::from_located(expr)) else {
        return;
    };
    if xxxxxxxx.patch(check.kind.code()) {
        if let DiagnosticKind::TypeOfPrimitive(violations::TypeOfPrimitive(primitive)) = &check.kind
        {
            check.amend(Fix::replacement(
                primitive.builtin(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
    }
    xxxxxxxx.diagnostics.push(check);
}
