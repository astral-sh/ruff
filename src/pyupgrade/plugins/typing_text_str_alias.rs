use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::pyupgrade::checks;

/// UP002
pub fn typing_text_str_alias(checker: &mut Checker, expr: &Expr) {
    let location = Range::from_located(expr);
    let Some(mut check) = checks::typing_text_str_alias(checker, expr, location) else {
        return;
    };
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            "str".to_string(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}
