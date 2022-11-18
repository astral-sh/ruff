use rustpython_ast::{Expr, Stmt};

use crate::ast::helpers;
use crate::check_ast::Checker;
use crate::pyupgrade;
use crate::pyupgrade::checks;

/// U008
pub fn super_call_with_parameters(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `check_super_args` too, so this is just an optimization.)
    if helpers::is_super_call_with_arguments(func, args) {
        let scope = checker.current_scope();
        let parents: Vec<&Stmt> = checker
            .parent_stack
            .iter()
            .map(|index| checker.parents[*index])
            .collect();
        if let Some(mut check) = checks::super_args(scope, &parents, expr, func, args) {
            if checker.patch(check.kind.code()) {
                if let Some(fix) = pyupgrade::fixes::remove_super_arguments(checker.locator, expr) {
                    check.amend(fix);
                }
            }
            checker.add_check(check)
        }
    }
}
