use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::{self, Constant, Expr};

use ruff_python_semantic::model::SemanticModel;

static PASSWORD_CANDIDATE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(^|_)(?i)(pas+wo?r?d|pass(phrase)?|pwd|token|secrete?)($|_)").unwrap()
});

pub(crate) fn string_literal(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(string),
            ..
        }) => Some(string),
        _ => None,
    }
}

pub(crate) fn matches_password_name(string: &str) -> bool {
    PASSWORD_CANDIDATE_REGEX.is_match(string)
}

pub(crate) fn is_untyped_exception(type_: Option<&Expr>, model: &SemanticModel) -> bool {
    type_.map_or(true, |type_| {
        if let Expr::Tuple(ast::ExprTuple { elts, .. }) = &type_ {
            elts.iter().any(|type_| {
                model.resolve_call_path(type_).map_or(false, |call_path| {
                    call_path.as_slice() == ["", "Exception"]
                        || call_path.as_slice() == ["", "BaseException"]
                })
            })
        } else {
            model.resolve_call_path(type_).map_or(false, |call_path| {
                call_path.as_slice() == ["", "Exception"]
                    || call_path.as_slice() == ["", "BaseException"]
            })
        }
    })
}
