use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::checkers::ast::Checker;

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

pub fn is_untyped_exception(type_: Option<&Expr>, checker: &Checker) -> bool {
    type_.map_or(true, |type_| {
        if let ExprKind::Tuple { elts, .. } = &type_.node {
            elts.iter().any(|type_| {
                checker.resolve_call_path(type_).map_or(false, |call_path| {
                    call_path.as_slice() == ["", "Exception"]
                        || call_path.as_slice() == ["", "BaseException"]
                })
            })
        } else {
            checker.resolve_call_path(type_).map_or(false, |call_path| {
                call_path.as_slice() == ["", "Exception"]
                    || call_path.as_slice() == ["", "BaseException"]
            })
        }
    })
}
