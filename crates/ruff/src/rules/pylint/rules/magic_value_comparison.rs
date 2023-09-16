use itertools::Itertools;
use ruff_python_ast::{self as ast, Constant, Expr, UnaryOp};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::settings::ConstantType;

/// ## What it does
/// Checks for the use of unnamed numerical constants ("magic") values in
/// comparisons.
///
/// ## Why is this bad?
/// The use of "magic" values can make code harder to read and maintain, as
/// readers will have to infer the meaning of the value from the context.
/// Such values are discouraged by [PEP 8].
///
/// For convenience, this rule excludes a variety of common values from the
/// "magic" value definition, such as `0`, `1`, `""`, and `"__main__"`.
///
/// ## Example
/// ```python
/// def calculate_discount(price: float) -> float:
///     return price * (1 - 0.2)
/// ```
///
/// Use instead:
/// ```python
/// DISCOUNT_RATE = 0.2
///
///
/// def calculate_discount(price: float) -> float:
///     return price * (1 - DISCOUNT_RATE)
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#constants
#[violation]
pub struct MagicValueComparison {
    value: String,
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
    match expr {
        Expr::Constant(ast::ExprConstant { value, .. }) => Some(value),
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert,
            operand,
            range: _,
        }) => match operand.as_ref() {
            Expr::Constant(ast::ExprConstant { value, .. }) => Some(value),
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
        Constant::Str(ast::StringConstant { value, .. }) => {
            !matches!(value.as_str(), "" | "__main__")
        }
        Constant::Int(value) => !matches!(value.try_into(), Ok(0 | 1)),
        Constant::Bytes(_) => true,
        Constant::Float(_) => true,
        Constant::Complex { .. } => true,
    }
}

/// PLR2004
pub(crate) fn magic_value_comparison(checker: &mut Checker, left: &Expr, comparators: &[Expr]) {
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
                        value: checker.generator().expr(comparison_expr),
                    },
                    comparison_expr.range(),
                ));
            }
        }
    }
}
