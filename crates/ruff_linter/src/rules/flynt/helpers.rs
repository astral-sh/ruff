use ruff_python_ast::{self as ast, Arguments, Constant, ConversionFlag, Expr};
use ruff_text_size::TextRange;

/// Wrap an expression in a `FormattedValue` with no special formatting.
fn to_formatted_value_expr(inner: &Expr) -> ast::FStringPart {
    ast::FStringPart::FormattedValue(ast::FormattedValue {
        expression: Box::new(inner.clone()),
        debug_text: None,
        conversion: ConversionFlag::None,
        format_spec: vec![],
        range: TextRange::default(),
    })
}

/// Convert a string to a constant string expression.
pub(super) fn to_constant_string(s: &str) -> ast::FStringPart {
    ast::FStringPart::Literal(ast::PartialString {
        value: s.to_owned(),
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
pub(super) fn to_fstring_part(expr: &Expr) -> Option<ast::FStringPart> {
    match expr {
        // These are directly handled by `unparse_f_string_element`:
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(value),
            range,
        }) => Some(ast::FStringPart::Literal(ast::PartialString {
            value: value.to_string(),
            range: *range,
        })),
        // These should be pretty safe to wrap in a formatted value.
        Expr::Constant(ast::ExprConstant {
            value:
                Constant::Int(_) | Constant::Float(_) | Constant::Bool(_) | Constant::Complex { .. },
            ..
        })
        | Expr::Name(_)
        | Expr::Attribute(_) => Some(to_formatted_value_expr(expr)),
        Expr::Call(_) if is_simple_call(expr) => Some(to_formatted_value_expr(expr)),
        _ => None,
    }
}
