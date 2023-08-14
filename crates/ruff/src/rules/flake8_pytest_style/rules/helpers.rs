use ruff_python_ast::{self as ast, Constant, Decorator, Expr, Keyword};

use ruff_python_ast::call_path::{collect_call_path, CallPath};
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::PythonWhitespace;

pub(super) fn get_mark_decorators(
    decorators: &[Decorator],
) -> impl Iterator<Item = (&Decorator, CallPath)> {
    decorators.iter().filter_map(|decorator| {
        let Some(call_path) = collect_call_path(map_callable(&decorator.expression)) else {
            return None;
        };
        if call_path.len() > 2 && call_path.as_slice()[..2] == ["pytest", "mark"] {
            Some((decorator, call_path))
        } else {
            None
        }
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
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(ast::StringConstant { value, .. }),
        ..
    }) = &keyword.value
    {
        value == literal
    } else {
        false
    }
}

pub(super) fn is_empty_or_null_string(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(string),
            ..
        }) => string.is_empty(),
        Expr::Constant(constant) if constant.value.is_none() => true,
        Expr::FString(ast::ExprFString { values, .. }) => {
            values.iter().all(is_empty_or_null_string)
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
