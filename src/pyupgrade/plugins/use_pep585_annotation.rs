use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// UP006
pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr, id: &str) {
    let replacement = *checker.import_aliases.get(id).unwrap_or(&id);
    let mut check = Check::new(
        CheckKind::UsePEP585Annotation(replacement.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            replacement.to_lowercase(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}
