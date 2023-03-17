use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::checkers::ast::Checker;

static PASSWORD_CANDIDATE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|_)(?i)(pas+wo?r?d|pass(phrase)?|pwd|token|secrete?)($|_)").unwrap()
});

pub fn string_literal(expr: &Expr) -> Option<&str> {
    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => Some(string),
        _ => None,
    }
}

pub fn matches_password_name(string: &str) -> bool {
    PASSWORD_CANDIDATE_REGEX.is_match(string)
}

pub fn is_untyped_exception(type_: Option<&Expr>, checker: &Checker) -> bool {
    type_.map_or(true, |type_| {
        if let ExprKind::Tuple { elts, .. } = &type_.node {
            elts.iter().any(|type_| {
                checker
                    .ctx
                    .resolve_call_path(type_)
                    .map_or(false, |call_path| {
                        call_path.as_slice() == ["", "Exception"]
                            || call_path.as_slice() == ["", "BaseException"]
                    })
            })
        } else {
            checker
                .ctx
                .resolve_call_path(type_)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["", "Exception"]
                        || call_path.as_slice() == ["", "BaseException"]
                })
        }
    })
}
