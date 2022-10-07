use rustpython_ast::Expr;

use crate::ast::checks;
use crate::ast::types::{CheckLocator, Range};
use crate::autofix::fixer;
use crate::check_ast::Checker;
use crate::checks::{CheckKind, Fix};

pub fn type_of_primitive(checker: &mut Checker, expr: &Expr, func: &Expr, args: &Vec<Expr>) {
    if let Some(mut check) =
        checks::check_type_of_primitive(func, args, checker.locate_check(Range::from_located(expr)))
    {
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            if let CheckKind::TypeOfPrimitive(primitive) = &check.kind {
                check.amend(Fix {
                    content: primitive.builtin(),
                    location: expr.location,
                    end_location: expr.end_location,
                    applied: false,
                });
            }
        }
        checker.add_check(check);
    }
}
