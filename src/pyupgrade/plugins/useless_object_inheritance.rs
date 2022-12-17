use rustpython_ast::{Expr, Keyword, Stmt};

use crate::checkers::ast::Checker;
use crate::pyupgrade;
use crate::pyupgrade::checks;

/// UP004
pub fn useless_object_inheritance(
    checker: &mut Checker,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
) {
    let Some(mut check) = checks::useless_object_inheritance(name, bases, checker.current_scope(), &checker.bindings) else {
        return;
    };
    if checker.patch(check.kind.code()) {
        if let Some(fix) = pyupgrade::fixes::remove_class_def_base(
            checker.locator,
            stmt.location,
            check.location,
            bases,
            keywords,
        ) {
            check.amend(fix);
        }
    }
    checker.add_check(check);
}
