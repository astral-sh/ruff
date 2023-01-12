use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

fn is_magic_value(constant: &Constant) -> bool {
    match constant {
        Constant::None => false,
        // E712 `if True == do_something():`
        Constant::Bool(_) => false,
        Constant::Str(value) => !matches!(value.as_str(), "" | "__main__"),
        Constant::Bytes(_) => true,
        Constant::Int(value) => !matches!(value.try_into(), Ok(-1 | 0 | 1)),
        Constant::Tuple(_) => true,
        Constant::Float(_) => true,
        Constant::Complex { .. } => true,
        Constant::Ellipsis => true,
    }
}

/// PLR2004
pub fn magic_value_comparison(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    comparators: &[Expr],
) {
    let mut diagnostics = vec![];

    for comparison_expr in comparators.iter().chain([left]) {
        if let ExprKind::Constant { value, .. } = &comparison_expr.node {
            if is_magic_value(value) {
                let diagnostic = Diagnostic::new(
                    violations::MagicValueComparison(value.to_string()),
                    Range::from_located(expr),
                );

                diagnostics.push(diagnostic);
            }
        }
    }

    // If all of the comparators (`+ 1` includes `left`) are constant skip rule.
    // R0133: comparison-of-constants
    if comparators.len() + 1 == diagnostics.len() {
        return;
    }

    for d in diagnostics {
        checker.diagnostics.push(d);
    }
}
