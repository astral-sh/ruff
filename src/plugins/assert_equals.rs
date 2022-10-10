use rustpython_ast::{Expr, Location};

use crate::ast::checks;
use crate::autofix::fixer;
use crate::check_ast::Checker;
use crate::checks::Fix;

pub fn assert_equals(checker: &mut Checker, expr: &Expr) {
    if let Some(mut check) = checks::check_assert_equals(expr) {
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            check.amend(Fix {
                content: "self.assertEqual".to_string(),
                location: Location::new(expr.location.row(), expr.location.column()),
                end_location: Location::new(expr.end_location.row(), expr.end_location.column()),
                applied: false,
            });
        }
        checker.add_check(check);
    }
}
