use once_cell::sync::Lazy;
use regex::Regex;
use ruff_python_ast::{self as ast, Constant, Expr};

use ruff_python_semantic::SemanticModel;

static PASSWORD_CANDIDATE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|_)(?i)(pas+wo?r?d|pass(phrase)?|pwd|token|secrete?)($|_)").unwrap()
});

pub(super) fn string_literal(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(string),
            ..
        }) => Some(string),
        _ => None,
    }
}

pub(super) fn matches_password_name(string: &str) -> bool {
    PASSWORD_CANDIDATE_REGEX.is_match(string)
}

pub(super) fn is_untyped_exception(type_: Option<&Expr>, semantic: &SemanticModel) -> bool {
    type_.map_or(true, |type_| {
        if let Expr::Tuple(ast::ExprTuple { elts, .. }) = &type_ {
            elts.iter().any(|type_| {
                semantic.resolve_call_path(type_).is_some_and(|call_path| {
                    matches!(call_path.as_slice(), ["", "Exception" | "BaseException"])
                })
            })
        } else {
            semantic.resolve_call_path(type_).is_some_and(|call_path| {
                matches!(call_path.as_slice(), ["", "Exception" | "BaseException"])
            })
        }
    })
}
