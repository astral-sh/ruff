use ruff_python_ast::helpers::create_expr;
use rustpython_parser::ast::{Constant, Expr, ExprKind};

fn to_formatted_value_expr(inner: Expr) -> Expr {
    create_expr(ExprKind::FormattedValue {
        value: Box::new(inner),
        conversion: 0,
        format_spec: None,
    })
}

/// Figure out if `expr` represents a "simple" call
/// (i.e. one that can be safely converted to a formatted value).
fn is_simple_call(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Call {
            func,
            args,
            keywords,
        } => args.is_empty() && keywords.is_empty() && is_simple_callee(func),
        _ => false,
    }
}

/// Figure out if `func` represents a "simple" callee (a bare name, or a chain of simple
/// attribute accesses).
fn is_simple_callee(func: &Expr) -> bool {
    match &func.node {
        ExprKind::Name { .. } => true,
        ExprKind::Attribute { value, .. } => is_simple_callee(value),
        _ => false,
    }
}

/// Convert an expression to a f-string element (if it looks like a good idea).
pub fn to_fstring_elem(expr: Expr) -> Option<Expr> {
    match &expr.node {
        // These are directly handled by `unparse_fstring_elem`:
        ExprKind::Constant {
            value: Constant::Str(_),
            ..
        }
        | ExprKind::JoinedStr { .. }
        | ExprKind::FormattedValue { .. } => Some(expr),
        // These should be pretty safe to wrap in a formatted value.
        ExprKind::Constant {
            value:
                Constant::Int(_) | Constant::Float(_) | Constant::Bool(_) | Constant::Complex { .. },
            ..
        }
        | ExprKind::Name { .. }
        | ExprKind::Attribute { .. } => Some(to_formatted_value_expr(expr)),
        ExprKind::Call { .. } if is_simple_call(&expr) => Some(to_formatted_value_expr(expr)),
        _ => None,
    }
}

/// Convert a string to a constant string expression.
pub fn to_constant_string(s: &str) -> Expr {
    create_expr(ExprKind::Constant {
        value: Constant::Str(s.to_owned()),
        kind: None,
    })
}
