use rustpython_parser::ast::{Expr, ExprKind};
use smallvec::smallvec;
use std::fmt::Display;

/// A representation of a qualified name, like `typing.List`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPath<'a>(smallvec::SmallVec<[&'a str; 8]>);

impl<'a> CallPath<'a> {
    /// Create a new, empty [`CallPath`].
    pub fn new() -> Self {
        Self(smallvec![])
    }

    /// Create a new, empty [`CallPath`] with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(smallvec::SmallVec::with_capacity(capacity))
    }

    /// Create a [`CallPath`] from an expression.
    pub fn try_from_expr(expr: &'a Expr) -> Option<Self> {
        let mut segments = CallPath::new();
        collect_call_path(expr, &mut segments).then_some(segments)
    }

    /// Create a [`CallPath`] from a fully-qualified name.
    ///
    /// ```rust
    /// # use smallvec::smallvec;
    /// # use ruff_python_ast::call_path::{CallPath, from_qualified_name};
    ///
    /// assert_eq!(CallPath::from_qualified_name("typing.List").as_slice(), ["typing", "List"]);
    /// assert_eq!(CallPath::from_qualified_name("list").as_slice(), ["", "list"]);
    /// ```
    pub fn from_qualified_name(name: &'a str) -> Self {
        Self(if name.contains('.') {
            name.split('.').collect()
        } else {
            // Special-case: for builtins, return `["", "int"]` instead of `["int"]`.
            smallvec!["", name]
        })
    }

    /// Create a [`CallPath`] from an unqualified name.
    ///
    /// ```rust
    /// # use smallvec::smallvec;
    /// # use ruff_python_ast::call_path::{CallPath, from_unqualified_name};
    ///
    /// assert_eq!(CallPath::from_unqualified_name("typing.List").as_slice(), ["typing", "List"]);
    /// assert_eq!(CallPath::from_unqualified_name("list").as_slice(), ["list"]);
    /// ```
    pub fn from_unqualified_name(name: &'a str) -> Self {
        Self(name.split('.').collect())
    }

    pub fn push(&mut self, segment: &'a str) {
        self.0.push(segment)
    }

    pub fn pop(&mut self) -> Option<&'a str> {
        self.0.pop()
    }

    pub fn extend<I: IntoIterator<Item = &'a str>>(&mut self, iter: I) {
        self.0.extend(iter)
    }

    pub fn first(&self) -> Option<&&'a str> {
        self.0.first()
    }

    pub fn last(&self) -> Option<&&'a str> {
        self.0.last()
    }

    pub fn as_slice(&self) -> &[&str] {
        self.0.as_slice()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }
}

impl<'a> IntoIterator for CallPath<'a> {
    type Item = &'a str;
    type IntoIter = smallvec::IntoIter<[&'a str; 8]>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Display for CallPath<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format_call_path(self.as_slice()))
    }
}

/// Collect a [`CallPath`] from an [`Expr`].
fn collect_call_path<'a>(expr: &'a Expr, parts: &mut CallPath<'a>) -> bool {
    match &expr.node {
        ExprKind::Attribute { value, attr, .. } => {
            if collect_call_path(value, parts) {
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

/// Format a [`CallPath`] for display.
fn format_call_path(call_path: &[&str]) -> String {
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
