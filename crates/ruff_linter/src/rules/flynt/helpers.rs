use ruff_python_ast::{BuildId, Builder, Expr, ExprKind};
use ruff_text_size::Ranged;

use crate::fix::syntax;

/// Figure out if `expr` represents a "simple" call
/// (i.e. one that can be safely converted to a formatted value).
fn is_simple_call(expr: Expr<'_>) -> bool {
    match expr.classify() {
        ExprKind::Call(expr_call) => {
            let func = expr_call.func();
            let arguments = expr_call.arguments();

            let args = arguments.args();
            let keywords = arguments.keywords();
            args.is_empty() && keywords.is_empty() && is_simple_callee(func)
        }
        _ => false,
    }
}

/// Figure out if `func` represents a "simple" callee (a bare name, or a chain of simple
/// attribute accesses).
fn is_simple_callee(func: Expr<'_>) -> bool {
    match func.classify() {
        ExprKind::Name(_) => true,
        ExprKind::Attribute(expr_attribute) => {
            let value = expr_attribute.value();

            is_simple_callee(value)
        }
        _ => false,
    }
}

/// Convert an expression to an f-string or t-string element (if it looks like a good idea).
pub(super) fn to_interpolated_string_element(
    builder: &mut Builder,
    expr: Expr<'_>,
) -> Option<BuildId<ruff_python_ast::kind::InterpolatedStringElement>> {
    match expr.classify() {
        ExprKind::StringLiteral(expr_string_literal) => Some(syntax::literal_element(
            builder,
            expr_string_literal.value().to_str(),
            expr_string_literal.range(),
        )),
        // These should be pretty safe to wrap in a formatted value.
        ExprKind::NumberLiteral(_)
        | ExprKind::BooleanLiteral(_)
        | ExprKind::Name(_)
        | ExprKind::Attribute(_) => Some(syntax::interpolation(builder, expr)),
        ExprKind::Call(_) if is_simple_call(expr) => Some(syntax::interpolation(builder, expr)),
        _ => None,
    }
}
