use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use rustpython_ast::{Expr, ExprKind};

define_violation!(
    pub struct UnnecessaryParenOnRaiseException;
);
impl Violation for UnnecessaryParenOnRaiseException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary parentheses on raised exception")
    }
}

/// RSE102
pub fn unnecessary_paren_on_raise_exception(checker: &mut Checker, expr: &Expr) {
    match &expr.node {
        ExprKind::Call { args, keywords, .. } if args.is_empty() && keywords.is_empty() => {
            checker.diagnostics.push(Diagnostic::new(
                UnnecessaryParenOnRaiseException,
                Range::from_located(expr),
            ));
        }
        _ => (),
    }
}
