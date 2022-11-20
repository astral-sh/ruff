use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::pyupgrade::checks;

/// U003
pub fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let Some(mut check) = checks::type_of_primitive(func, args, Range::from_located(expr)) {
        if checker.patch(check.kind.code()) {
            if let CheckKind::TypeOfPrimitive(primitive) = &check.kind {
                check.amend(Fix::replacement(
                    primitive.builtin(),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
        }
        checker.add_check(check);
    }
}
