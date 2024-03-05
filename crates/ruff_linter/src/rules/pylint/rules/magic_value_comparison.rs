use itertools::Itertools;
use ruff_python_ast::{self as ast, Expr, Int, LiteralExpressionRef, UnaryOp};

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
/// def apply_discount(price: float) -> float:
///     if price <= 100:
///         return price / 2
///     else:
///         return price
/// ```
///
/// Use instead:
/// ```python
/// MAX_DISCOUNT = 100
///
///
/// def apply_discount(price: float) -> float:
///     if price <= MAX_DISCOUNT:
///         return price / 2
///     else:
///         return price
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
            "Magic value used in comparison, consider replacing `{value}` with a constant variable"
        )
    }
}

/// If an [`Expr`] is a literal (or unary operation on a literal), return the [`LiteralExpressionRef`].
fn as_literal(expr: &Expr) -> Option<LiteralExpressionRef<'_>> {
    match expr {
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::UAdd | UnaryOp::USub | UnaryOp::Invert,
            operand,
            ..
        }) => operand.as_literal_expr(),
        _ => expr.as_literal_expr(),
    }
}

fn is_magic_value(literal_expr: LiteralExpressionRef, allowed_types: &[ConstantType]) -> bool {
    if let Some(constant_type) = ConstantType::try_from_literal_expr(literal_expr) {
        if allowed_types.contains(&constant_type) {
            return false;
        }
    }

    match literal_expr {
        // Ignore `None`, `Bool`, and `Ellipsis` constants.
        LiteralExpressionRef::NoneLiteral(_)
        | LiteralExpressionRef::BooleanLiteral(_)
        | LiteralExpressionRef::EllipsisLiteral(_) => false,
        // Special-case some common string and integer types.
        LiteralExpressionRef::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            !matches!(value.to_str(), "" | "__main__")
        }
        LiteralExpressionRef::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => match value {
            #[allow(clippy::float_cmp)]
            ast::Number::Float(value) => !(*value == 0.0 || *value == 1.0),
            ast::Number::Int(value) => !matches!(*value, Int::ZERO | Int::ONE),
            ast::Number::Complex { .. } => true,
        },
        LiteralExpressionRef::BytesLiteral(_) => true,
    }
}

/// PLR2004
pub(crate) fn magic_value_comparison(checker: &mut Checker, left: &Expr, comparators: &[Expr]) {
    for (left, right) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows()
    {
        // If both of the comparators are literals, skip rule for the whole expression.
        // R0133: comparison-of-constants
        if as_literal(left).is_some() && as_literal(right).is_some() {
            return;
        }
    }

    for comparison_expr in std::iter::once(left).chain(comparators.iter()) {
        if let Some(value) = as_literal(comparison_expr) {
            if is_magic_value(value, &checker.settings.pylint.allow_magic_value_types) {
                checker.diagnostics.push(Diagnostic::new(
                    MagicValueComparison {
                        value: checker.locator().slice(comparison_expr).to_string(),
                    },
                    comparison_expr.range(),
                ));
            }
        }
    }
}
