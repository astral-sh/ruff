use num_traits::identities::Zero;
use ruff_python_ast::call_path::{collect_call_path, CallPath};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use ruff_python_ast::helpers::map_callable;

use crate::checkers::ast::Checker;

const ITERABLE_INITIALIZERS: &[&str] = &["dict", "frozenset", "list", "tuple", "set"];

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

pub(super) fn is_pytest_fail(call: &Expr, checker: &Checker) -> bool {
    checker
        .ctx
        .resolve_call_path(call)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "fail"]
        })
}

pub(super) fn is_pytest_fixture(decorator: &Expr, checker: &Checker) -> bool {
    checker
        .ctx
        .resolve_call_path(if let ExprKind::Call { func, .. } = &decorator.node {
            func
        } else {
            decorator
        })
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "fixture"]
        })
}

pub(super) fn is_pytest_yield_fixture(decorator: &Expr, checker: &Checker) -> bool {
    checker
        .ctx
        .resolve_call_path(map_callable(decorator))
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "yield_fixture"]
        })
}

pub(super) fn is_abstractmethod_decorator(decorator: &Expr, checker: &Checker) -> bool {
    checker
        .ctx
        .resolve_call_path(decorator)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["abc", "abstractmethod"]
        })
}

/// Check if the expression is a constant that evaluates to false.
pub(super) fn is_falsy_constant(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant { value, .. } => match value {
            Constant::Bool(value) => !*value,
            Constant::None => true,
            Constant::Str(string) => string.is_empty(),
            Constant::Bytes(bytes) => bytes.is_empty(),
            Constant::Int(int) => int.is_zero(),
            Constant::Float(float) => *float == 0.0,
            Constant::Complex { real, imag } => *real == 0.0 && *imag == 0.0,
            Constant::Ellipsis => true,
            Constant::Tuple(elts) => elts.is_empty(),
        },
        ExprKind::JoinedStr { values, .. } => values.is_empty(),
        ExprKind::Tuple { elts, .. } | ExprKind::List { elts, .. } => elts.is_empty(),
        ExprKind::Dict { keys, .. } => keys.is_empty(),
        ExprKind::Call {
            func,
            args,
            keywords,
        } => {
            if let ExprKind::Name { id, .. } = &func.node {
                if ITERABLE_INITIALIZERS.contains(&id.as_str()) {
                    if args.is_empty() && keywords.is_empty() {
                        return true;
                    } else if !keywords.is_empty() {
                        return false;
                    } else if let Some(arg) = args.get(0) {
                        return is_falsy_constant(arg);
                    }
                    return false;
                }
                false
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(super) fn is_pytest_parametrize(decorator: &Expr, checker: &Checker) -> bool {
    checker
        .ctx
        .resolve_call_path(map_callable(decorator))
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pytest", "mark", "parametrize"]
        })
}

pub(super) fn keyword_is_literal(kw: &Keyword, literal: &str) -> bool {
    if let ExprKind::Constant {
        value: Constant::Str(string),
        ..
    } = &kw.node.value.node
    {
        string == literal
    } else {
        false
    }
}

pub(super) fn is_empty_or_null_string(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant {
            value: Constant::Str(string),
            ..
        } => string.is_empty(),
        ExprKind::Constant {
            value: Constant::None,
            ..
        } => true,
        ExprKind::JoinedStr { values } => values.iter().all(is_empty_or_null_string),
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
