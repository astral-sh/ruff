use itertools::Itertools;
use ruff_python_ast::{self as ast, Expr, Int, LiteralExpressionRef, UnaryOp};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::settings::ConstantType;

#[violation]
pub struct NanComparison {
    value: String,
}

impl Violation for NanComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Nan comparison")
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

/// PLW0177
pub(crate) fn nan_comparison(checker: &mut Checker, left: &Expr, comparators: &[Expr]) {
    for (left, right) in std::iter::once(left)
        .chain(comparators.iter())
        .tuple_windows()
    {
        if let Some(name_left) = checker.semantic().resolve_qualified_name(left) {
            println!("{}", name_left);
        }
        // let name_right = checker.semantic().resolve_qualified_name(right).unwrap();

    }

    // for comparison_expr in std::iter::once(left).chain(comparators.iter()) {
    //     if let Some(value) = as_literal(comparison_expr) {
    //         if is_magic_value(value, &checker.settings.pylint.allow_magic_value_types) {
    //             checker.diagnostics.push(Diagnostic::new(
    //                 NanComparison {
    //                     value: checker.locator().slice(comparison_expr).to_string(),
    //                 },
    //                 comparison_expr.range(),
    //             ));
    //         }
    //     }
    // }
}
