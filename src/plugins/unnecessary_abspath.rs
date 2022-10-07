use rustpython_ast::Expr;

use crate::ast::checks;
use crate::ast::types::{CheckLocator, Range};
use crate::autofix::fixer;
use crate::check_ast::Checker;
use crate::checks::Fix;

pub fn unnecessary_abspath(checker: &mut Checker, expr: &Expr, func: &Expr, args: &Vec<Expr>) {
    if let Some(mut check) = checks::check_unnecessary_abspath(
        func,
        args,
        checker.locate_check(Range::from_located(expr)),
    ) {
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            check.amend(Fix {
                content: "__file__".to_string(),
                location: expr.location,
                end_location: expr.end_location,
                applied: false,
            });
        }
        checker.add_check(check);
    }
}
