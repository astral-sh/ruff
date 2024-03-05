use ruff_python_ast::Stmt;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Scope, ScopeKind};

use crate::rules::pep8_naming::settings::IgnoreNames;

/// ## What it does
/// Checks for functions with "dunder" names (that is, names with two
/// leading and trailing underscores) that are not documented.
///
/// ## Why is this bad?
/// [PEP 8] recommends that only documented "dunder" methods are used:
///
/// > ..."magic" objects or attributes that live in user-controlled
/// > namespaces. E.g. `__init__`, `__import__` or `__file__`. Never invent
/// > such names; only use them as documented.
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
#[violation]
pub struct DunderFunctionName;

impl Violation for DunderFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function name should not start and end with `__`")
    }
}

/// N807
pub(crate) fn dunder_function_name(
    scope: &Scope,
    stmt: &Stmt,
    name: &str,
    ignore_names: &IgnoreNames,
) -> Option<Diagnostic> {
    if matches!(scope.kind, ScopeKind::Class(_)) {
        return None;
    }
    if !visibility::is_magic(name) {
        return None;
    }
    // Allowed under PEP 562 (https://peps.python.org/pep-0562/).
    if matches!(scope.kind, ScopeKind::Module) && (name == "__getattr__" || name == "__dir__") {
        return None;
    }
    // Ignore any explicitly-allowed names.
    if ignore_names.matches(name) {
        return None;
    }
    Some(Diagnostic::new(DunderFunctionName, stmt.identifier()))
}
