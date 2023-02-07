use rustpython_parser::ast::{Constant, Expr, ExprKind};

const PASSWORD_NAMES: [&str; 7] = [
    "password", "pass", "passwd", "pwd", "secret", "token", "secrete",
];

pub fn string_literal(expr: &Expr) -> Option<&str> {
    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => Some(string),
        _ => None,
    }
}

// Maybe use regex for this?
pub fn matches_password_name(string: &str) -> bool {
    PASSWORD_NAMES
        .iter()
        .any(|name| string.to_lowercase().contains(name))
}
