use rustpython_ast::{Expr, Keyword, Stmt};

use crate::check_ast::Checker;
use crate::pyupgrade;
use crate::pyupgrade::checks;

pub fn useless_object_inheritance(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
) {
    let scope = checker.current_scope();
    if let Some(mut check) = checks::useless_object_inheritance(name, bases, scope) {
        if checker.patch() {
            if let Some(fix) = pyupgrade::fixes::remove_class_def_base(
                checker.locator,
                &stmt.location,
                check.location,
                bases,
                keywords,
            ) {
                check.amend(fix);
            }
        }
        checker.add_check(check);
    }
}
