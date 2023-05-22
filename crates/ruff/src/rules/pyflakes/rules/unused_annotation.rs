use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::ScopeId;

use crate::checkers::ast::Checker;

#[violation]
pub struct UnusedAnnotation {
    name: String,
}

impl Violation for UnusedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedAnnotation { name } = self;
        format!("Local variable `{name}` is annotated but never used")
    }
}

/// F842
pub(crate) fn unused_annotation(checker: &mut Checker, scope: ScopeId) {
    let scope = &checker.semantic_model().scopes[scope];

    let bindings: Vec<_> = scope
        .bindings()
        .filter_map(|(name, index)| {
            let name = *name;
            let binding = &checker.semantic_model().bindings[*index];

            if !binding.used()
                && binding.kind.is_annotation()
                && !checker.settings.dummy_variable_rgx.is_match(name)
            {
                Some((name.to_string(), binding.range))
            } else {
                None
            }
        })
        .collect();

    for (name, range) in bindings {
        checker.diagnostics.push(Diagnostic::new(
            UnusedAnnotation {
                name: (*name).to_string(),
            },
            range,
        ));
    }
}
