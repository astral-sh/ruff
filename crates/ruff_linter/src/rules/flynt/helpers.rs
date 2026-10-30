use ruff_python_ast::{self as ast, ConversionFlag, Expr, flat};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

/// Wrap an expression in a [`ast::FStringElement::Expression`] with no special formatting.
fn to_interpolated_string_interpolation_element(
    inner: flat::View<'_, Expr>,
) -> ast::InterpolatedStringElement {
    ast::InterpolatedStringElement::Interpolation(ast::InterpolatedElement {
        expression: Box::new(inner.to_owned_node()),
        debug_text: None,
        conversion: ConversionFlag::None,
        format_spec: None,
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    })
}

/// Convert a string to a [`ast::InterpolatedStringLiteralElement `].
pub(super) fn to_interpolated_string_literal_element(s: &str) -> ast::InterpolatedStringElement {
    ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
        value: Box::from(s),
        range: TextRange::default(),
        node_index: ruff_python_ast::NodeIndex::NONE,
    })
}

/// Figure out if `expr` represents a "simple" call
/// (i.e. one that can be safely converted to a formatted value).
fn is_simple_call(expr: flat::View<'_, Expr>) -> bool {
    match expr.classify() {
        flat::ExprKind::Call(expr_call) => {
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
fn is_simple_callee(func: flat::View<'_, Expr>) -> bool {
    match func.classify() {
        flat::ExprKind::Name(_) => true,
        flat::ExprKind::Attribute(expr_attribute) => {
            let value = expr_attribute.value();

            is_simple_callee(value)
        }
        _ => false,
    }
}

/// Convert an expression to an f-string or t-string element (if it looks like a good idea).
pub(super) fn to_interpolated_string_element(
    expr: flat::View<'_, Expr>,
) -> Option<ast::InterpolatedStringElement> {
    match expr.classify() {
        flat::ExprKind::StringLiteral(expr_string_literal) => {
            let value = expr_string_literal.value();
            let range = expr_string_literal.range();
            Some(ast::InterpolatedStringElement::Literal(
                ast::InterpolatedStringLiteralElement {
                    value: value.to_string().into_boxed_str(),
                    range,
                    node_index: ast::NodeIndex::NONE,
                },
            ))
        }
        // These should be pretty safe to wrap in a formatted value.
        flat::ExprKind::NumberLiteral(_)
        | flat::ExprKind::BooleanLiteral(_)
        | flat::ExprKind::Name(_)
        | flat::ExprKind::Attribute(_) => Some(to_interpolated_string_interpolation_element(expr)),
        flat::ExprKind::Call(_) if is_simple_call(expr) => {
            Some(to_interpolated_string_interpolation_element(expr))
        }
        _ => None,
    }
}
