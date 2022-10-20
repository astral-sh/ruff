use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::{Range, Scope, ScopeKind};
use crate::checks::{Check, CheckKind};

pub fn invalid_class_name(class_def: &Stmt, name: &str) -> Option<Check> {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    if !stripped
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        || stripped.contains('_')
    {
        return Some(Check::new(
            CheckKind::InvalidClassName(name.to_string()),
            Range::from_located(class_def),
        ));
    }
    None
}

pub fn invalid_function_name(func_def: &Stmt, name: &str) -> Option<Check> {
    if name.chars().any(|c| c.is_uppercase()) {
        return Some(Check::new(
            CheckKind::InvalidFunctionName(name.to_string()),
            Range::from_located(func_def),
        ));
    }
    None
}

pub fn invalid_argument_name(location: Range, name: &str) -> Option<Check> {
    if name.chars().any(|c| c.is_uppercase()) {
        return Some(Check::new(
            CheckKind::InvalidArgumentName(name.to_string()),
            location,
        ));
    }
    None
}

pub fn invalid_first_argument_name_for_class_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            id == "classmethod"
        } else {
            false
        }
    }) {
        if let Some(arg) = args.args.first() {
            if arg.node.arg != "cls" {
                return Some(Check::new(
                    CheckKind::InvalidFirstArgumentNameForClassMethod,
                    Range::from_located(arg),
                ));
            }
        }
    }
    None
}

pub fn invalid_first_argument_name_for_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            id == "classmethod" || id == "staticmethod"
        } else {
            false
        }
    }) {
        return None;
    }

    if let Some(arg) = args.args.first() {
        if arg.node.arg != "self" {
            return Some(Check::new(
                CheckKind::InvalidFirstArgumentNameForMethod,
                Range::from_located(arg),
            ));
        }
    }
    None
}

pub fn dunder_function_name(func_def: &Stmt, scope: &Scope, name: &str) -> Option<Check> {
    if matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if name.starts_with("__") && name.ends_with("__") {
        return Some(Check::new(
            CheckKind::DunderFunctionName,
            Range::from_located(func_def),
        ));
    }

    None
}
