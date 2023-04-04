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
pub fn collect_call_path(expr: &Expr) -> Option<CallPath> {
    let mut segments = smallvec![];
    collect_call_path_inner(expr, &mut segments).then_some(segments)
}

/// Convert an `Expr` to its call path (like `List`, or `typing.List`).
pub fn compose_call_path(expr: &Expr) -> Option<String> {
    collect_call_path(expr).map(|call_path| format_call_path(&call_path))
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

/// Create a [`CallPath`] from an unqualified name.
///
/// ```rust
/// # use smallvec::smallvec;
/// # use ruff_python_ast::call_path::from_unqualified_name;
///
/// assert_eq!(from_unqualified_name("typing.List").as_slice(), ["typing", "List"]);
/// assert_eq!(from_unqualified_name("list").as_slice(), ["list"]);
/// ```
pub fn from_unqualified_name(name: &str) -> CallPath {
    name.split('.').collect()
}

/// Create a [`CallPath`] from a fully-qualified name.
///
/// ```rust
/// # use smallvec::smallvec;
/// # use ruff_python_ast::call_path::from_qualified_name;
///
/// assert_eq!(from_qualified_name("typing.List").as_slice(), ["typing", "List"]);
/// assert_eq!(from_qualified_name("list").as_slice(), ["", "list"]);
/// ```
pub fn from_qualified_name(name: &str) -> CallPath {
    if name.contains('.') {
        name.split('.').collect()
    } else {
        // Special-case: for builtins, return `["", "int"]` instead of `["int"]`.
        smallvec!["", name]
    }
}
