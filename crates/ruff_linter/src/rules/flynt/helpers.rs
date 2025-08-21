use ruff_python_ast::{self as ast, Arguments, ConversionFlag, Expr};
use ruff_text_size::TextRange;

/// Wrap an expression in a [`ast::FStringElement::Expression`] with no special formatting.
fn to_interpolated_string_interpolation_element(inner: &Expr) -> ast::InterpolatedStringElement {
    ast::InterpolatedStringElement::Interpolation(ast::InterpolatedElement {
        expression: Box::new(inner.clone()),
        debug_text: None,
        conversion: ConversionFlag::None,
        format_spec: None,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
    })
}

/// Convert a string to a [`ast::InterpolatedStringLiteralElement `].
pub(super) fn to_interpolated_string_literal_element(s: &str) -> ast::InterpolatedStringElement {
    ast::InterpolatedStringElement::Literal(ast::InterpolatedStringLiteralElement {
        value: Box::from(s),
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
    })
}

/// Figure out if `expr` represents a "simple" call
/// (i.e. one that can be safely converted to a formatted value).
fn is_simple_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call(ast::ExprCall {
            func,
            arguments:
                Arguments {
                    args,
                    keywords,
                    range: _,
                    node_index: _,
                },
            range: _,
            node_index: _,
        }) => args.is_empty() && keywords.is_empty() && is_simple_callee(func),
        _ => false,
    }
}

/// Figure out if `func` represents a "simple" callee (a bare name, or a chain of simple
/// attribute accesses).
fn is_simple_callee(func: &Expr) -> bool {
    match func {
        Expr::Name(_) => true,
        Expr::Attribute(ast::ExprAttribute { value, .. }) => is_simple_callee(value),
        _ => false,
    }
}

/// Convert an expression to an f-string or t-string element (if it looks like a good idea).
pub(super) fn to_interpolated_string_element(
    expr: &Expr,
) -> Option<ast::InterpolatedStringElement> {
    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral {
            value,
            range,
            node_index,
        }) => Some(ast::InterpolatedStringElement::Literal(
            ast::InterpolatedStringLiteralElement {
                value: value.to_string().into_boxed_str(),
                range: *range,
                node_index: node_index.clone(),
            },
        )),
        // These should be pretty safe to wrap in a formatted value.
        Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) | Expr::Name(_) | Expr::Attribute(_) => {
            Some(to_interpolated_string_interpolation_element(expr))
        }
        Expr::Call(_) if is_simple_call(expr) => {
            Some(to_interpolated_string_interpolation_element(expr))
        }
        _ => None,
    }
}
