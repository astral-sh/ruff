use rustpython_ast::{Expr, Stmt};

use crate::ast::checks;
use crate::autofix::{fixer, fixes};
use crate::check_ast::Checker;

pub fn super_call_with_parameters(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &Vec<Expr>,
) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `check_super_args` too, so this is just an optimization.)
    if checks::is_super_call_with_arguments(func, args) {
        let scope = checker.current_scope();
        let parents: Vec<&Stmt> = checker
            .parent_stack
            .iter()
            .map(|index| checker.parents[*index])
            .collect();
        if let Some(mut check) = checks::check_super_args(scope, &parents, expr, func, args) {
            if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
                if let Some(fix) = fixes::remove_super_arguments(&mut checker.locator, expr) {
                    check.amend(fix);
                }
            }
            checker.add_check(check)
        }
    }
}
