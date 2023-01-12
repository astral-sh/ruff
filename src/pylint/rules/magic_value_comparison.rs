use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

fn is_magic_value(constant: &Constant) -> Option<String> {
    match constant {
        Constant::Int(value) => {
            if matches!(value.try_into(), Ok(-1 | 0 | 1)) {
                None
            } else {
                Some(value.to_string())
            }
        }
        Constant::Float(value) => Some(value.to_string()),
        _ => None,
    }
}

/// PLR2004
pub fn magic_value_comparison(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    comparators: &[Expr],
) {
    for comparison_expr in comparators.iter().chain([left]) {
        if let ExprKind::Constant { value, .. } = &comparison_expr.node {
            if let Some(value) = is_magic_value(value) {
                let diagnostic = Diagnostic::new(
                    violations::MagicValueComparison(value),
                    Range::from_located(expr),
                );

                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
