use ruff_python_ast::{self as ast, Arguments, ConversionFlag, Expr};
use ruff_text_size::TextRange;

/// Wrap an expression in a [`ast::FStringElement::Expression`] with no special formatting.
fn to_f_string_expression_element(inner: &Expr) -> ast::FStringElement {
    ast::FStringElement::Expression(ast::FStringExpressionElement {
        expression: Box::new(inner.clone()),
        debug_text: None,
        conversion: ConversionFlag::None,
        format_spec: None,
        range: TextRange::default(),
    })
}

/// Convert a string to a [`ast::FStringElement::Literal`].
pub(super) fn to_f_string_literal_element(s: &str) -> ast::FStringElement {
    ast::FStringElement::Literal(ast::FStringLiteralElement {
        value: Box::from(s),
        range: TextRange::default(),
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
                },
            range: _,
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

/// Convert an expression to a f-string element (if it looks like a good idea).
pub(super) fn to_f_string_element(expr: &Expr) -> Option<ast::FStringElement> {
    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value, range }) => {
            Some(ast::FStringElement::Literal(ast::FStringLiteralElement {
                value: value.to_string().into_boxed_str(),
                range: *range,
            }))
        }
        // These should be pretty safe to wrap in a formatted value.
        Expr::NumberLiteral(_) | Expr::BooleanLiteral(_) | Expr::Name(_) | Expr::Attribute(_) => {
            Some(to_f_string_expression_element(expr))
        }
        Expr::Call(_) if is_simple_call(expr) => Some(to_f_string_expression_element(expr)),
        _ => None,
    }
}
