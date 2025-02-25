use regex::Regex;
use ruff_python_ast::{self as ast, Expr};
use std::sync::LazyLock;

use ruff_python_semantic::SemanticModel;

static PASSWORD_CANDIDATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|_)(?i)(pas+wo?r?d|pass(phrase)?|pwd|token|secrete?)($|_)").unwrap()
});

pub(super) fn string_literal(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => Some(value.to_str()),
        _ => None,
    }
}

pub(super) fn matches_password_name(string: &str) -> bool {
    PASSWORD_CANDIDATE_REGEX.is_match(string)
}

pub(super) fn is_untyped_exception(type_: Option<&Expr>, semantic: &SemanticModel) -> bool {
    type_.is_none_or(|type_| {
        if let Expr::Tuple(ast::ExprTuple { elts, .. }) = &type_ {
            elts.iter().any(|type_| {
                semantic
                    .resolve_builtin_symbol(type_)
                    .is_some_and(|builtin| matches!(builtin, "Exception" | "BaseException"))
            })
        } else {
            semantic
                .resolve_builtin_symbol(type_)
                .is_some_and(|builtin| matches!(builtin, "Exception" | "BaseException"))
        }
    })
}
