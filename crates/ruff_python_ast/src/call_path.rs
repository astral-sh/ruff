use rustpython_parser::ast::{Expr, ExprKind};
use smallvec::smallvec;

/// A representation of a qualified name, like `typing.List`.
pub type CallPath<'a> = smallvec::SmallVec<[&'a str; 8]>;

fn collect_call_path_inner<'a>(expr: &'a Expr, parts: &mut CallPath<'a>) -> bool {
    match &expr.node {
        ExprKind::Attribute { value, attr, .. } => {
            if collect_call_path_inner(value, parts) {
                parts.push(attr);
                true
            } else {
                false
            }
        }
        ExprKind::Name { id, .. } => {
            parts.push(id);
            true
        }
        _ => false,
    }
}

/// Convert an `Expr` to its [`CallPath`] segments (like `["typing", "List"]`).
pub fn collect_call_path(expr: &Expr) -> CallPath {
    let mut segments = smallvec![];
    collect_call_path_inner(expr, &mut segments);
    segments
}

/// Convert an `Expr` to its call path (like `List`, or `typing.List`).
pub fn compose_call_path(expr: &Expr) -> Option<String> {
    let call_path = collect_call_path(expr);
    if call_path.is_empty() {
        None
    } else {
        Some(format_call_path(&call_path))
    }
}

/// Format a call path for display.
pub fn format_call_path(call_path: &[&str]) -> String {
    if call_path
        .first()
        .expect("Unable to format empty call path")
        .is_empty()
    {
        call_path[1..].join(".")
    } else {
        call_path.join(".")
    }
}

/// Split a fully-qualified name (like `typing.List`) into (`typing`, `List`).
pub fn to_call_path(target: &str) -> CallPath {
    if target.contains('.') {
        target.split('.').collect()
    } else {
        // Special-case: for builtins, return `["", "int"]` instead of `["int"]`.
        smallvec!["", target]
    }
}
