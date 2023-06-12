use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, ConversionFlag, Expr};

/// Wrap an expression in a `FormattedValue` with no special formatting.
fn to_formatted_value_expr(inner: &Expr) -> Expr {
    let node = ast::ExprFormattedValue {
        value: Box::new(inner.clone()),
        conversion: ConversionFlag::None,
        format_spec: None,
        range: TextRange::default(),
    };
    node.into()
}

/// Convert a string to a constant string expression.
pub(super) fn to_constant_string(s: &str) -> Expr {
    let node = ast::ExprConstant {
        value: Constant::Str(s.to_owned()),
        kind: None,
        range: TextRange::default(),
    };
    node.into()
}

/// Figure out if `expr` represents a "simple" call
/// (i.e. one that can be safely converted to a formatted value).
fn is_simple_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
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
pub(super) fn to_fstring_elem(expr: &Expr) -> Option<Expr> {
    match expr {
        // These are directly handled by `unparse_fstring_elem`:
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(_),
            ..
        })
        | Expr::JoinedStr(_)
        | Expr::FormattedValue(_) => Some(expr.clone()),
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
