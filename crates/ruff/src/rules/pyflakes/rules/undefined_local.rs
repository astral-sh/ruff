use std::string::ToString;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
#[violation]
pub struct UndefinedLocal {
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
pub(crate) fn undefined_local(checker: &mut Checker, name: &str) {
    // If the name hasn't already been defined in the current scope...
    let current = checker.semantic().scope();
    if !current.kind.is_any_function() || current.has(name) {
        return;
    }

    let Some(parent) = current.parent else {
        return;
    };

    // For every function and module scope above us...
    let local_access = checker
        .semantic()
        .scopes
        .ancestors(parent)
        .find_map(|scope| {
            if !(scope.kind.is_any_function() || scope.kind.is_module()) {
                return None;
            }

            // If the name was defined in that scope...
            if let Some(binding) = scope
                .get(name)
                .map(|binding_id| checker.semantic().binding(binding_id))
            {
                // And has already been accessed in the current scope...
                if let Some(range) = binding.references().find_map(|reference_id| {
                    let reference = checker.semantic().reference(reference_id);
                    if checker.semantic().is_current_scope(reference.scope_id()) {
                        Some(reference.range())
                    } else {
                        None
                    }
                }) {
                    // Then it's probably an error.
                    return Some(range);
                }
            }

            None
        });

    if let Some(location) = local_access {
        checker.diagnostics.push(Diagnostic::new(
            UndefinedLocal {
                name: name.to_string(),
            },
            location,
        ));
    }
}
