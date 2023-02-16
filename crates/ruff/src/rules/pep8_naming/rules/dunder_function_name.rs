use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Stmt;

use crate::ast::helpers::identifier_range;
use crate::ast::types::{Scope, ScopeKind};
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for functions with "dunder" names (that is, names with two
    /// leading and trailing underscores) that are not documented.
    ///
    /// ## Why is this bad?
    /// [PEP 8] recommends that only documented "dunder" methods are used:
    ///
    /// > ..."magic" objects or attributes that live in user-controlled
    /// > namespaces. E.g. `__init__`, `__import__` or `__file__`. Never invent
    /// such names; only use them as documented.
    ///
    /// ## Example
    /// ```python
    /// def __my_function__():
    ///     pass
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def my_function():
    ///     pass
    /// ```
    ///
    /// [PEP 8]: https://peps.python.org/pep-0008/
    pub struct DunderFunctionName;
);
impl Violation for DunderFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function name should not start and end with `__`")
    }
}

/// N807
pub fn dunder_function_name(
    scope: &Scope,
    stmt: &Stmt,
    name: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if matches!(scope.kind, ScopeKind::Class(_)) {
        return None;
    }
    if !(name.starts_with("__") && name.ends_with("__")) {
        return None;
    }
    // Allowed under PEP 562 (https://peps.python.org/pep-0562/).
    if matches!(scope.kind, ScopeKind::Module) && (name == "__getattr__" || name == "__dir__") {
        return None;
    }

    Some(Diagnostic::new(
        DunderFunctionName,
        identifier_range(stmt, locator),
    ))
}
