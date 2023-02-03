use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;

define_simple_violation!(
    UnnecessaryDirectLambdaCall,
    "Lambda expression called directly. Execute the expression inline instead."
);

/// PLC3002
pub fn unnecessary_direct_lambda_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Lambda { .. } = &func.node {
        checker.diagnostics.push(Diagnostic::new(
            UnnecessaryDirectLambdaCall,
            Range::from_located(expr),
        ));
    }
}
