use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLC3002
pub fn unnecessary_direct_lambda_call(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Lambda { .. } = &func.node {
        checker.add_check(Check::new(
            CheckKind::UnnecessaryDirectLambdaCall,
            Range::from_located(expr),
        ));
    }
}
