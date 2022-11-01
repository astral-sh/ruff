use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Location, StmtKind};

fn compose_call_path_inner<'a>(expr: &'a Expr, parts: &mut Vec<&'a str>) {
    match &expr.node {
        ExprKind::Call { func, .. } => {
            compose_call_path_inner(func, parts);
        }
        ExprKind::Attribute { value, attr, .. } => {
            compose_call_path_inner(value, parts);
            parts.push(attr);
        }
        ExprKind::Name { id, .. } => {
            parts.push(id);
        }
        _ => {}
    }
}

pub fn compose_call_path(expr: &Expr) -> Option<String> {
    let mut segments = vec![];
    compose_call_path_inner(expr, &mut segments);
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

/// Return `true` if the `Expr` is a name or attribute reference to `${target}`.
pub fn match_name_or_attr(expr: &Expr, target: &str) -> bool {
    match &expr.node {
        ExprKind::Attribute { attr, .. } => target == attr,
        ExprKind::Name { id, .. } => target == id,
        _ => false,
    }
}

/// Return `true` if the `Expr` is a reference to `${module}.${target}`.
///
/// Useful for, e.g., ensuring that a `Union` reference represents `typing.Union`.
pub fn match_name_or_attr_from_module(
    expr: &Expr,
    target: &str,
    module: &str,
    imports: Option<&BTreeSet<&str>>,
) -> bool {
    match &expr.node {
        ExprKind::Attribute { value, attr, .. } => match &value.node {
            ExprKind::Name { id, .. } => id == module && target == attr,
            _ => false,
        },
        ExprKind::Name { id, .. } => {
            target == id
                && imports
                    .map(|imports| imports.contains(&id.as_str()))
                    .unwrap_or_default()
        }
        _ => false,
    }
}

static DUNDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"__[^\s]+__").unwrap());

pub fn is_assignment_to_a_dunder(node: &StmtKind) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type annotation.
    // This is what pycodestyle (as of 2.9.1) does.
    match node {
        StmtKind::Assign {
            targets,
            value: _,
            type_comment: _,
        } => {
            if targets.len() != 1 {
                return false;
            }
            match &targets[0].node {
                ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
                _ => false,
            }
        }
        StmtKind::AnnAssign {
            target,
            annotation: _,
            value: _,
            simple: _,
        } => match &target.node {
            ExprKind::Name { id, ctx: _ } => DUNDER_REGEX.is_match(id),
            _ => false,
        },
        _ => false,
    }
}

/// Extract the names of all handled exceptions.
pub fn extract_handler_names(handlers: &[Excepthandler]) -> Vec<String> {
    let mut handler_names = vec![];
    for handler in handlers {
        match &handler.node {
            ExcepthandlerKind::ExceptHandler { type_, .. } => {
                if let Some(type_) = type_ {
                    if let ExprKind::Tuple { elts, .. } = &type_.node {
                        for type_ in elts {
                            if let Some(name) = compose_call_path(type_) {
                                handler_names.push(name);
                            }
                        }
                    } else if let Some(name) = compose_call_path(type_) {
                        handler_names.push(name);
                    }
                }
            }
        }
    }
    handler_names
}

/// Returns `true` if a call is an argumented `super` invocation.
pub fn is_super_call_with_arguments(func: &Expr, args: &[Expr]) -> bool {
    // Check: is this a `super` call?
    if let ExprKind::Name { id, .. } = &func.node {
        id == "super" && !args.is_empty()
    } else {
        false
    }
}

/// Convert a location within a file (relative to `base`) to an absolute position.
pub fn to_absolute(relative: &Location, base: &Location) -> Location {
    if relative.row() == 1 {
        Location::new(
            relative.row() + base.row() - 1,
            relative.column() + base.column(),
        )
    } else {
        Location::new(relative.row() + base.row() - 1, relative.column())
    }
}
