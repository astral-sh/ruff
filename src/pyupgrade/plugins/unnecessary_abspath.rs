use rustpython_ast::Expr;

use crate::ast::types::{CheckLocator, Range};
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::pyupgrade::checks;

pub fn unnecessary_abspath(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let Some(mut check) =
        checks::unnecessary_abspath(func, args, checker.locate_check(Range::from_located(expr)))
    {
        if checker.patch() {
            check.amend(Fix::replacement(
                "__file__".to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
