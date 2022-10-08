use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::fixer;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind, Fix};
use crate::python::typing;

pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr, id: &str) {
    // TODO(charlie): Verify that the builtin is imported from the `typing` module.
    if typing::is_pep585_builtin(id) {
        let mut check = Check::new(
            CheckKind::UsePEP585Annotation(id.to_string()),
            Range::from_located(expr),
        );
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            check.amend(Fix {
                content: id.to_lowercase(),
                location: expr.location,
                end_location: expr.end_location,
                applied: false,
            })
        }
        checker.add_check(check);
    }
}
