use std::string::ToString;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

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
    let current = checker.semantic_model().scope();
    if !current.kind.is_function() || current.defines(name) {
        return;
    }

    let Some(parent) = current.parent else {
        return;
    };

    // For every function and module scope above us...
    let local_access = checker
        .semantic_model()
        .scopes
        .ancestors(parent)
        .find_map(|scope| {
            if !(scope.kind.is_function() || scope.kind.is_module()) {
                return None;
            }

            // If the name was defined in that scope...
            if let Some(binding) = scope
                .get(name)
                .map(|binding_id| &checker.semantic_model().bindings[*binding_id])
            {
                // And has already been accessed in the current scope...
                if let Some(range) = binding.references.iter().find_map(|reference_id| {
                    let reference = checker.semantic_model().references.resolve(*reference_id);
                    if reference.scope_id() == checker.semantic_model().scope_id {
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
