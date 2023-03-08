use itertools::Itertools;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Unaryop};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::rules::pylint::settings::ConstantType;

#[violation]
pub struct MagicValueComparison {
    pub value: String,
}

impl Violation for MagicValueComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MagicValueComparison { value } = self;
        format!(
            "Magic value used in comparison, consider replacing {value} with a constant variable"
        )
    }
}

/// If an [`Expr`] is a constant (or unary operation on a constant), return the [`Constant`].
fn as_constant(expr: &Expr) -> Option<&Constant> {
    match &expr.node {
        ExprKind::Constant { value, .. } => Some(value),
        ExprKind::UnaryOp {
            op: Unaryop::UAdd | Unaryop::USub | Unaryop::Invert,
            operand,
        } => match &operand.node {
            ExprKind::Constant { value, .. } => Some(value),
            _ => None,
        },
        _ => None,
    }
}

/// Return `true` if a [`Constant`] is a magic value.
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
        Constant::Int(value) => !matches!(value.try_into(), Ok(0 | 1)),
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
        if as_constant(left).is_some() && as_constant(right).is_some() {
            return;
        }
    }

    for comparison_expr in std::iter::once(left).chain(comparators.iter()) {
        if let Some(value) = as_constant(comparison_expr) {
            if is_magic_value(value, &checker.settings.pylint.allow_magic_value_types) {
                checker.diagnostics.push(Diagnostic::new(
                    MagicValueComparison {
                        value: unparse_expr(comparison_expr, checker.stylist),
                    },
                    Range::from(comparison_expr),
                ));
            }
        }
    }
}
