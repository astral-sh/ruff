use rustpython_ast::{Expr, Stmt};

use crate::ast::helpers;
use crate::pyupgrade;
use crate::pyupgrade::checks;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP008
pub fn super_call_with_parameters(
    xxxxxxxx: &mut xxxxxxxx,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `check_super_args` too, so this is just an optimization.)
    if !helpers::is_super_call_with_arguments(func, args) {
        return;
    }
    let scope = xxxxxxxx.current_scope();
    let parents: Vec<&Stmt> = xxxxxxxx
        .parents
        .iter()
        .map(std::convert::Into::into)
        .collect();
    let Some(mut check) = checks::super_args(scope, &parents, expr, func, args) else {
        return;
    };
    if xxxxxxxx.patch(check.kind.code()) {
        if let Some(fix) = pyupgrade::fixes::remove_super_arguments(xxxxxxxx.locator, expr) {
            check.amend(fix);
        }
    }
    xxxxxxxx.diagnostics.push(check);
}
