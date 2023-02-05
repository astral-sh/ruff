use crate::define_violation;
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, Stmt};

use super::super::fixes;
use crate::ast::helpers;
use crate::checkers::ast::Checker;

define_violation!(
    pub struct SuperCallWithParameters;
);
impl AlwaysAutofixableViolation for SuperCallWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `super()` instead of `super(__class__, self)`")
    }

    fn autofix_title(&self) -> String {
        "Remove `__super__` parameters".to_string()
    }
}

/// UP008
pub fn super_call_with_parameters(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    // Only bother going through the super check at all if we're in a `super` call.
    // (We check this in `check_super_args` too, so this is just an optimization.)
    if !helpers::is_super_call_with_arguments(func, args) {
        return;
    }
    let scope = checker.current_scope();
    let parents: Vec<&Stmt> = checker
        .parents
        .iter()
        .map(std::convert::Into::into)
        .collect();
    let Some(mut diagnostic) = super::super_args(scope, &parents, expr, func, args) else {
        return;
    };
    if checker.patch(diagnostic.kind.rule()) {
        if let Some(fix) = fixes::remove_super_arguments(checker.locator, checker.stylist, expr) {
            diagnostic.amend(fix);
        }
    }
    checker.diagnostics.push(diagnostic);
}
