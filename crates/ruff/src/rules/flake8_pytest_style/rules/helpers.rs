use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Keyword};

use ruff_python_ast::call_path::{collect_call_path, CallPath};
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::context::Context;

pub(super) fn get_mark_decorators(decorators: &[Expr]) -> impl Iterator<Item = (&Expr, CallPath)> {
    decorators.iter().filter_map(|decorator| {
        let Some(call_path) = collect_call_path(map_callable(decorator)) else {
            return None;
        };
        if call_path.len() > 2 && call_path.as_slice()[..2] == ["pytest", "mark"] {
            Some((decorator, call_path))
        } else {
            None
        }
    })
}

pub(super) fn is_pytest_fail(context: &Context, call: &Expr) -> bool {
    context.resolve_call_path(call).map_or(false, |call_path| {
        call_path.as_slice() == ["pytest", "fail"]
    })
}

pub(super) fn is_pytest_fixture(context: &Context, decorator: &Expr) -> bool {
    context
        .resolve_call_path(map_callable(decorator))
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "fixture"]
        })
}

pub(super) fn is_pytest_yield_fixture(context: &Context, decorator: &Expr) -> bool {
    context
        .resolve_call_path(map_callable(decorator))
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "yield_fixture"]
        })
}

pub(super) fn is_pytest_parametrize(context: &Context, decorator: &Expr) -> bool {
    context
        .resolve_call_path(map_callable(decorator))
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "mark", "parametrize"]
        })
}

pub(super) fn keyword_is_literal(kw: &Keyword, literal: &str) -> bool {
    if let ExprKind::Constant(ast::ExprConstant {
        value: Constant::Str(string),
        ..
    }) = &kw.node.value.node
    {
        string == literal
    } else {
        false
    }
}

pub(super) fn is_empty_or_null_string(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant(ast::ExprConstant {
            value: Constant::Str(string),
            ..
        }) => string.is_empty(),
        ExprKind::Constant(ast::ExprConstant {
            value: Constant::None,
            ..
        }) => true,
        ExprKind::JoinedStr(ast::ExprJoinedStr { values }) => {
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
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<&str>>()
}
