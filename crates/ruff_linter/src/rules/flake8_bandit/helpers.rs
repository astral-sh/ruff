use regex::Regex;
use ruff_python_ast::Expr;
use ruff_python_ast::flat;
use std::sync::LazyLock;

use ruff_python_semantic::SemanticModel;

static PASSWORD_CANDIDATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|_)(?i)(pas+wo?r?d|pass(phrase)?|pwd|token|secrete?)($|_)").unwrap()
});

pub(super) fn string_literal(expr: flat::View<'_, Expr>) -> Option<&str> {
    match expr.classify() {
        flat::ExprKind::StringLiteral(expr_string_literal) => {
            let value = expr_string_literal.value();

            Some(value.to_str())
        }
        _ => None,
    }
}

pub(super) fn matches_password_name(string: &str) -> bool {
    PASSWORD_CANDIDATE_REGEX.is_match(string)
}

pub(super) fn is_untyped_exception(
    type_: Option<flat::View<'_, Expr>>,
    semantic: &SemanticModel,
) -> bool {
    type_.is_none_or(|type_| {
        if let flat::ExprKind::Tuple(tuple) = type_.classify() {
            let elts = tuple.elts();
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
