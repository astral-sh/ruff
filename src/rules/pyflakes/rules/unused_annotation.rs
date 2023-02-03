use crate::ast::types::BindingKind;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;

use crate::violation::Violation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct UnusedAnnotation {
        pub name: String,
    }
);
impl Violation for UnusedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedAnnotation { name } = self;
        format!("Local variable `{name}` is annotated but never used")
    }
}

/// F842
pub fn unused_annotation(checker: &mut Checker, scope: usize) {
    let scope = &checker.scopes[scope];
    for (name, binding) in scope
        .bindings
        .iter()
        .map(|(name, index)| (name, &checker.bindings[*index]))
    {
        if !binding.used()
            && matches!(binding.kind, BindingKind::Annotation)
            && !checker.settings.dummy_variable_rgx.is_match(name)
        {
            checker.diagnostics.push(Diagnostic::new(
                UnusedAnnotation {
                    name: (*name).to_string(),
                },
                binding.range,
            ));
        }
    }
}
