use itertools::Itertools;
use ruff_python_ast::{self as ast, Expr, LiteralExpressionRef, UnaryOp};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::pylint::settings::{AllowedValue, ConstantType};

/// ## What it does
/// Checks for the use of unnamed hard-coded ("magic") values in comparisons.
///
/// ## Why is this bad?
/// The use of magic values can make code harder to read and maintain, as
/// readers will have to infer the meaning of the value from the context.
/// Such values are discouraged by [PEP 8] and should be replaced with variables
/// or named constants.
///
/// Some common values and object types are ignored by this rule be default.
/// These can be configured using the `lint.pylint.allow-magic-values` and
/// `lint.pylint.allow-magic-value-types` settings, respectively.
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
/// ## Options
/// - `lint.pylint.allow-magic-value-types`
/// - `lint.pylint.allow-magic-values`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#constants
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.221")]
pub(crate) struct MagicValueComparison {
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

fn is_magic_value(
    literal_expr: LiteralExpressionRef,
    allowed_types: &[ConstantType],
    allowed_values: &[AllowedValue],
) -> bool {
    // Check if the literal type is in the allowed types list
    if let Some(constant_type) = ConstantType::try_from_literal_expr(literal_expr) {
        if allowed_types.contains(&constant_type) {
            return false;
        }
    }

    // Check if the literal value is in the allowed values list
    if let Some(allowed_value) = AllowedValue::try_from_literal_expr(literal_expr) {
        if allowed_values.contains(&allowed_value) {
            return false;
        }
    }

    match literal_expr {
        // Ignore `None`, `Bool`, and `Ellipsis` constants.
        LiteralExpressionRef::NoneLiteral(_)
        | LiteralExpressionRef::BooleanLiteral(_)
        | LiteralExpressionRef::EllipsisLiteral(_) => false,
        LiteralExpressionRef::StringLiteral(_)
        | LiteralExpressionRef::NumberLiteral(_)
        | LiteralExpressionRef::BytesLiteral(_) => true,
    }
}

/// PLR2004
pub(crate) fn magic_value_comparison(checker: &Checker, left: &Expr, comparators: &[Expr]) {
    for (left, right) in std::iter::once(left).chain(comparators).tuple_windows() {
        // If both of the comparators are literals, skip rule for the whole expression.
        // R0133: comparison-of-constants
        if as_literal(left).is_some() && as_literal(right).is_some() {
            return;
        }
    }

    let allowed_types: &[ConstantType] = &checker.settings().pylint.allow_magic_value_types;
    let allowed_values: &[AllowedValue] = &checker.settings().pylint.allow_magic_values;

    for comparison_expr in std::iter::once(left).chain(comparators) {
        if let Some(value) = as_literal(comparison_expr) {
            if is_magic_value(value, allowed_types, allowed_values) {
                checker.report_diagnostic(
                    MagicValueComparison {
                        value: checker.locator().slice(comparison_expr).to_string(),
                    },
                    comparison_expr.range(),
                );
            }
        }
    }
}
