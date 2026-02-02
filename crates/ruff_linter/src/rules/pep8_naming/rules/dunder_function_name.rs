use ruff_python_ast::Stmt;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::identifier::Identifier;
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Scope, ScopeKind};

use crate::Violation;
use crate::checkers::ast::Checker;
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
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.82")]
pub(crate) struct DunderFunctionName;

impl Violation for DunderFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Function name should not start and end with `__`".to_string()
    }
}

/// N807
pub(crate) fn dunder_function_name(
    checker: &Checker,
    scope: &Scope,
    stmt: &Stmt,
    name: &str,
    ignore_names: &IgnoreNames,
) {
    if matches!(scope.kind, ScopeKind::Class(_)) {
        return;
    }
    if !visibility::is_magic(name) {
        return;
    }
    // Allowed under PEP 562 (https://peps.python.org/pep-0562/).
    if matches!(scope.kind, ScopeKind::Module) && (name == "__getattr__" || name == "__dir__") {
        return;
    }
    // Ignore any explicitly-allowed names.
    if ignore_names.matches(name) {
        return;
    }
    checker.report_diagnostic(DunderFunctionName, stmt.identifier());
}
