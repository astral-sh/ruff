use std::string::ToString;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::binding::Bindings;
use ruff_python_semantic::scope::{Scope, ScopeKind};

#[violation]
pub struct UndefinedLocal {
    pub name: String,
}

impl Violation for UndefinedLocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UndefinedLocal { name } = self;
        format!("Local variable `{name}` referenced before assignment")
    }
}

/// F821
pub fn undefined_local(name: &str, scopes: &[&Scope], bindings: &Bindings) -> Option<Diagnostic> {
    let current = &scopes.last().expect("No current scope found");
    if matches!(current.kind, ScopeKind::Function(_)) && !current.defines(name) {
        for scope in scopes.iter().rev().skip(1) {
            if matches!(scope.kind, ScopeKind::Function(_) | ScopeKind::Module) {
                if let Some(binding) = scope.get(name).map(|index| &bindings[*index]) {
                    if let Some((scope_id, location)) = binding.runtime_usage {
                        if scope_id == current.id {
                            return Some(Diagnostic::new(
                                UndefinedLocal {
                                    name: name.to_string(),
                                },
                                location,
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}
