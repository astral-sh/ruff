use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `nonlocal` names without bindings.
///
/// ## Why is this bad?
/// `nonlocal` names must be bound to a name in an outer scope.
/// Violating this rule leads to a `SyntaxError` at runtime.
///
/// ## Example
/// ```python
/// def foo():
///     def get_bar(self):
///         nonlocal bar
///         ...
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     bar = 1
///
///     def get_bar(self):
///         nonlocal bar
///         ...
/// ```
///
/// ## References
/// - [Python documentation: The `nonlocal` statement](https://docs.python.org/3/reference/simple_stmts.html#nonlocal)
/// - [PEP 3104 – Access to Names in Outer Scopes](https://peps.python.org/pep-3104/)
#[derive(ViolationMetadata)]
pub(crate) struct NonlocalWithoutBinding {
    pub(crate) name: String,
}

impl Violation for NonlocalWithoutBinding {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonlocalWithoutBinding { name } = self;
        format!("Nonlocal name `{name}` found without binding")
    }
}

/// PLE0117
pub(crate) fn nonlocal_without_binding(checker: &Checker, nonlocal: &ast::StmtNonlocal) {
    if !checker.semantic().scope_id.is_global() {
        for name in &nonlocal.names {
            // Skip __class__ in method definitions - it's implicitly available
            if name == "__class__" && is_in_method_definition(checker) {
                continue;
            }

            if checker.semantic().nonlocal(name).is_none() {
                checker.report_diagnostic(
                    NonlocalWithoutBinding {
                        name: name.to_string(),
                    },
                    name.range(),
                );
            }
        }
    }
}

/// Check if the current scope is within a method definition (function inside a class)
fn is_in_method_definition(checker: &Checker) -> bool {
    let semantic = checker.semantic();

    // Check if we're currently in a function scope
    if !matches!(semantic.current_scope().kind, ScopeKind::Function(_)) {
        return false;
    }

    // Walk up the scope hierarchy to find a class scope, skipping Type scopes
    let scopes: Vec<_> = semantic.current_scopes().collect();
    for scope in scopes.iter().skip(1) {
        // Skip current function scope
        match scope.kind {
            ScopeKind::Class(_) => return true,
            ScopeKind::Type => continue, // Skip Type scopes and keep looking
            _ => return false,
        }
    }

    false
}
