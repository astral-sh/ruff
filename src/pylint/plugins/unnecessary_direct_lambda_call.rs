use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLC3002
pub fn unnecessary_direct_lambda_call(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, func: &Expr) {
    if let ExprKind::Lambda { .. } = &func.node {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::UnnecessaryDirectLambdaCall,
            Range::from_located(expr),
        ));
    }
}
