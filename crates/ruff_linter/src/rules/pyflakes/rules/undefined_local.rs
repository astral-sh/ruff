use std::string::ToString;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{Scope, ScopeId};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for undefined local variables.
///
/// ## Why is this bad?
/// Referencing a local variable before it has been assigned will raise
/// an `UnboundLocalError` at runtime.
///
/// ## Example
/// ```python
/// x = 1
///
///
/// def foo():
///     x += 1
/// ```
///
/// Use instead:
/// ```python
/// x = 1
///
///
/// def foo():
///     global x
///     x += 1
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UndefinedLocal {
    name: String,
}

impl Violation for UndefinedLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocal { name } = self;
        format!("Local variable `{name}` referenced before assignment")
    }
}

/// F823
pub(crate) fn undefined_local(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    if scope.kind.is_function() {
        for (name, binding_id) in scope.bindings() {
            // If the variable shadows a binding in a parent scope...
            if let Some(shadowed_id) = checker.semantic().shadowed_binding(binding_id) {
                let shadowed = checker.semantic().binding(shadowed_id);
                // And that binding was referenced in the current scope...
                if let Some(range) = shadowed.references().find_map(|reference_id| {
                    let reference = checker.semantic().reference(reference_id);
                    if reference.scope_id() == scope_id {
                        Some(reference.range())
                    } else {
                        None
                    }
                }) {
                    // Then it's probably an error.
                    checker.report_diagnostic(Diagnostic::new(
                        UndefinedLocal {
                            name: name.to_string(),
                        },
                        range,
                    ));
                }
            }
        }
    }
}
