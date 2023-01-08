use rustpython_ast::{Expr, Keyword, Stmt};

use crate::pyupgrade;
use crate::pyupgrade::checks;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP004
pub fn useless_object_inheritance(
    xxxxxxxx: &mut xxxxxxxx,
    stmt: &Stmt,
    name: &str,
    bases: &[Expr],
    keywords: &[Keyword],
) {
    let Some(mut check) = checks::useless_object_inheritance(name, bases, xxxxxxxx.current_scope(), &xxxxxxxx.bindings) else {
        return;
    };
    if xxxxxxxx.patch(check.kind.code()) {
        if let Some(fix) = pyupgrade::fixes::remove_class_def_base(
            xxxxxxxx.locator,
            stmt.location,
            check.location,
            bases,
            keywords,
        ) {
            check.amend(fix);
        }
    }
    xxxxxxxx.diagnostics.push(check);
}
