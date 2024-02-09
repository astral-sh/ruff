use smallvec::{smallvec, SmallVec};

use crate::{nodes, Expr};

/// A representation of a qualified name, like `typing.List`.
pub type CallPath<'a> = SmallVec<[&'a str; 8]>;

/// Convert an `Expr` to its [`CallPath`] segments (like `["typing", "List"]`).
pub fn collect_call_path(expr: &Expr) -> Option<CallPath> {
    // Unroll the loop up to eight times, to match the maximum number of expected attributes.
    // In practice, unrolling appears to give about a 4x speed-up on this hot path.
    let attr1 = match expr {
        Expr::Attribute(attr1) => attr1,
        // Ex) `foo`
        Expr::Name(nodes::ExprName { id, .. }) => return Some(CallPath::from_slice(&[id])),
        _ => return None,
    };

    let attr2 = match attr1.value.as_ref() {
        Expr::Attribute(attr2) => attr2,
        // Ex) `foo.bar`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[id, &*attr1.attr]))
        }
        _ => return None,
    };

    let attr3 = match attr2.value.as_ref() {
        Expr::Attribute(attr3) => attr3,
        // Ex) `foo.bar.baz`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[id, &*attr2.attr, &*attr1.attr]));
        }
        _ => return None,
    };

    let attr4 = match attr3.value.as_ref() {
        Expr::Attribute(attr4) => attr4,
        // Ex) `foo.bar.baz.bop`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[
                id,
                &*attr3.attr,
                &*attr2.attr,
                &*attr1.attr,
            ]));
        }
        _ => return None,
    };

    let attr5 = match attr4.value.as_ref() {
        Expr::Attribute(attr5) => attr5,
        // Ex) `foo.bar.baz.bop.bap`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[
                id,
                &*attr4.attr,
                &*attr3.attr,
                &*attr2.attr,
                &*attr1.attr,
            ]));
        }
        _ => return None,
    };

    let attr6 = match attr5.value.as_ref() {
        Expr::Attribute(attr6) => attr6,
        // Ex) `foo.bar.baz.bop.bap.bab`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[
                id,
                &*attr5.attr,
                &*attr4.attr,
                &*attr3.attr,
                &*attr2.attr,
                &*attr1.attr,
            ]));
        }
        _ => return None,
    };

    let attr7 = match attr6.value.as_ref() {
        Expr::Attribute(attr7) => attr7,
        // Ex) `foo.bar.baz.bop.bap.bab.bob`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[
                id,
                &*attr6.attr,
                &*attr5.attr,
                &*attr4.attr,
                &*attr3.attr,
                &*attr2.attr,
                &*attr1.attr,
            ]));
        }
        _ => return None,
    };

    let attr8 = match attr7.value.as_ref() {
        Expr::Attribute(attr8) => attr8,
        // Ex) `foo.bar.baz.bop.bap.bab.bob.bib`
        Expr::Name(nodes::ExprName { id, .. }) => {
            return Some(CallPath::from_slice(&[
                id,
                &*attr7.attr,
                &*attr6.attr,
                &*attr5.attr,
                &*attr4.attr,
                &*attr3.attr,
                &*attr2.attr,
                &*attr1.attr,
            ]));
        }
        _ => return None,
    };

    collect_call_path(&attr8.value).map(|mut segments| {
        segments.extend([
            &*attr8.attr,
            &*attr7.attr,
            &*attr6.attr,
            &*attr5.attr,
            &*attr4.attr,
            &*attr3.attr,
            &*attr2.attr,
            &*attr1.attr,
        ]);
        segments
    })
}

/// Convert an `Expr` to its call path (like `List`, or `typing.List`).
pub fn compose_call_path(expr: &Expr) -> Option<String> {
    collect_call_path(expr).map(|call_path| format_call_path(&call_path))
}

/// Format a call path for display.
pub fn format_call_path(call_path: &[&str]) -> String {
    if call_path.first().map_or(false, |first| first.is_empty()) {
        // If the first segment is empty, the `CallPath` is that of a builtin.
        // Ex) `["", "bool"]` -> `"bool"`
        call_path[1..].join(".")
    } else if call_path
        .first()
        .map_or(false, |first| matches!(*first, "."))
    {
        // If the call path is dot-prefixed, it's an unresolved relative import.
        // Ex) `[".foo", "bar"]` -> `".foo.bar"`
        let mut formatted = String::new();
        let mut iter = call_path.iter();
        for segment in iter.by_ref() {
            if *segment == "." {
                formatted.push('.');
            } else {
                formatted.push_str(segment);
                break;
            }
        }
        for segment in iter {
            formatted.push('.');
            formatted.push_str(segment);
        }
        formatted
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
