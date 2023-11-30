use ruff_python_ast::{self as ast, Decorator, Expr, Keyword};

use ruff_python_ast::call_path::collect_call_path;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::PythonWhitespace;

pub(super) fn get_mark_decorators(
    decorators: &[Decorator],
) -> impl Iterator<Item = (&Decorator, &str)> {
    decorators.iter().filter_map(|decorator| {
        let Some(call_path) = collect_call_path(map_callable(&decorator.expression)) else {
            return None;
        };
        let ["pytest", "mark", marker] = call_path.as_slice() else {
            return None;
        };
        Some((decorator, *marker))
    })
}

pub(super) fn is_pytest_fail(call: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(call)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pytest", "fail"]))
}

pub(super) fn is_pytest_fixture(decorator: &Decorator, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(map_callable(&decorator.expression))
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pytest", "fixture"]))
}

pub(super) fn is_pytest_yield_fixture(decorator: &Decorator, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(map_callable(&decorator.expression))
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pytest", "yield_fixture"]))
}

pub(super) fn is_pytest_parametrize(decorator: &Decorator, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(map_callable(&decorator.expression))
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["pytest", "mark", "parametrize"]))
}

pub(super) fn keyword_is_literal(keyword: &Keyword, literal: &str) -> bool {
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &keyword.value {
        value == literal
    } else {
        false
    }
}

pub(super) fn is_empty_or_null_string(expr: &Expr) -> bool {
    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.is_empty(),
        Expr::NoneLiteral(_) => true,
        Expr::FString(ast::ExprFString { value, .. }) => {
            value.parts().all(|f_string_part| match f_string_part {
                ast::FStringPart::Literal(literal) => literal.is_empty(),
                ast::FStringPart::FString(f_string) => {
                    f_string.values.iter().all(is_empty_or_null_string)
                }
            })
        }
        _ => false,
    }
}

pub(super) fn split_names(names: &str) -> Vec<&str> {
    // Match the following pytest code:
    //    [x.strip() for x in argnames.split(",") if x.strip()]
    names
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim_whitespace();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<&str>>()
}
