use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr, id: &str) {
    let mut check = Check::new(
        CheckKind::UsePEP585Annotation(id.to_string()),
        Range::from_located(expr),
    );
    if checker.patch() {
        check.amend(Fix::replacement(
            id.to_lowercase(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}
