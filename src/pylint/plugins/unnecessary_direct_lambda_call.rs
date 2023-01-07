use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::{violations, Diagnostic};

/// PLC3002
pub fn unnecessary_direct_lambda_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Lambda { .. } = &func.node {
        checker.checks.push(Diagnostic::new(
            violations::UnnecessaryDirectLambdaCall,
            Range::from_located(expr),
        ));
    }
}
