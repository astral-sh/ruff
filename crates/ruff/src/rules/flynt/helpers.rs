use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Int};

use ruff_python_ast::helpers::create_expr;

/// Wrap an expression in a `FormattedValue` with no special formatting.
fn to_formatted_value_expr(inner: &Expr) -> Expr {
    create_expr(ast::ExprFormattedValue {
        value: Box::new(inner.clone()),
        conversion: Int::new(0),
        format_spec: None,
    })
}

/// Convert a string to a constant string expression.
pub(crate) fn to_constant_string(s: &str) -> Expr {
    create_expr(ast::ExprConstant {
        value: Constant::Str(s.to_owned()),
        kind: None,
    })
}

/// Figure out if `expr` represents a "simple" call
/// (i.e. one that can be safely converted to a formatted value).
fn is_simple_call(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Call(ast::ExprCall {
            func,
            args,
            keywords,
        }) => args.is_empty() && keywords.is_empty() && is_simple_callee(func),
        _ => false,
    }
}

/// Figure out if `func` represents a "simple" callee (a bare name, or a chain of simple
/// attribute accesses).
fn is_simple_callee(func: &Expr) -> bool {
    match &func.node {
        ExprKind::Name(_) => true,
        ExprKind::Attribute(ast::ExprAttribute { value, .. }) => is_simple_callee(value),
        _ => false,
    }
}

/// Convert an expression to a f-string element (if it looks like a good idea).
pub(crate) fn to_fstring_elem(expr: &Expr) -> Option<Expr> {
    match &expr.node {
        // These are directly handled by `unparse_fstring_elem`:
        ExprKind::Constant(ast::ExprConstant {
            value: Constant::Str(_),
            ..
        })
        | ExprKind::JoinedStr(_)
        | ExprKind::FormattedValue(_) => Some(expr.clone()),
        // These should be pretty safe to wrap in a formatted value.
        ExprKind::Constant(ast::ExprConstant {
            value:
                Constant::Int(_) | Constant::Float(_) | Constant::Bool(_) | Constant::Complex { .. },
            ..
        })
        | ExprKind::Name(_)
        | ExprKind::Attribute(_) => Some(to_formatted_value_expr(expr)),
        ExprKind::Call(_) if is_simple_call(expr) => Some(to_formatted_value_expr(expr)),
        _ => None,
    }
}
