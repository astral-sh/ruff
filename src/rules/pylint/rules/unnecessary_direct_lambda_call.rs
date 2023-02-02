use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct UnnecessaryDirectLambdaCall;
);
impl Violation for UnnecessaryDirectLambdaCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Lambda expression called directly. Execute the expression inline instead.")
    }
}

/// PLC3002
pub fn unnecessary_direct_lambda_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Lambda { .. } = &func.node {
        checker.diagnostics.push(Diagnostic::new(
            UnnecessaryDirectLambdaCall,
            Range::from_located(expr),
        ));
    }
}
