use crate::define_simple_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;

define_simple_violation!(NoEval, "No builtin `eval()` allowed");

/// PGH001 - no eval
pub fn no_eval(checker: &mut Checker, func: &Expr) {
    let ExprKind::Name { id, .. } = &func.node else {
        return;
    };
    if id != "eval" {
        return;
    }
    if !checker.is_builtin("eval") {
        return;
    }
    checker
        .diagnostics
        .push(Diagnostic::new(NoEval, Range::from_located(func)));
}
