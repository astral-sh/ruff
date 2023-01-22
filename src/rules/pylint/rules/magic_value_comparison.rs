use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::pylint::settings::ConstantType;
use crate::violations;

fn is_magic_value(constant: &Constant, allowed_types: &[ConstantType]) -> bool {
    if let Ok(constant_type) = ConstantType::try_from(constant) {
        if allowed_types.contains(&constant_type) {
            return false;
        }
    }
    match constant {
        // Ignore `None`, `Bool`, and `Ellipsis` constants.
        Constant::None => false,
        Constant::Bool(_) => false,
        Constant::Ellipsis => false,
        // Otherwise, special-case some common string and integer types.
        Constant::Str(value) => !matches!(value.as_str(), "" | "__main__"),
        Constant::Int(value) => !matches!(value.try_into(), Ok(-1 | 0 | 1)),
        Constant::Bytes(_) => true,
        Constant::Tuple(_) => true,
        Constant::Float(_) => true,
        Constant::Complex { .. } => true,
    }
}

/// PLR2004
pub fn magic_value_comparison(checker: &mut Checker, left: &Expr, comparators: &[Expr]) {
    for (left, right) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows()
    {
        // If both of the comparators are constant, skip rule for the whole expression.
        // R0133: comparison-of-constants
        if matches!(left.node, ExprKind::Constant { .. })
            && matches!(right.node, ExprKind::Constant { .. })
        {
            return;
        }
    }

    for comparison_expr in std::iter::once(left).chain(comparators.iter()) {
        if let ExprKind::Constant { value, .. } = &comparison_expr.node {
            if is_magic_value(value, &checker.settings.pylint.allow_magic_value_types) {
                checker.diagnostics.push(Diagnostic::new(
                    violations::MagicValueComparison {
                        value: value.to_string(),
                    },
                    Range::from_located(comparison_expr),
                ));
            }
        }
    }
}
