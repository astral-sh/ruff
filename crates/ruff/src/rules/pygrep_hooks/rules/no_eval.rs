use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct NoEval;
);
impl Violation for NoEval {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No builtin `eval()` allowed")
    }
}
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
