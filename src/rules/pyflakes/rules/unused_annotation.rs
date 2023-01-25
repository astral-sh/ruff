use crate::ast::types::BindingKind;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// F842
pub fn unused_annotation(checker: &mut Checker, scope: usize) {
    let scope = &checker.scopes[scope];
    for (name, binding) in scope
        .values
        .iter()
        .map(|(name, index)| (name, &checker.bindings[*index]))
    {
        if !binding.used()
            && matches!(binding.kind, BindingKind::Annotation)
            && !checker.settings.dummy_variable_rgx.is_match(name)
        {
            checker.diagnostics.push(Diagnostic::new(
                violations::UnusedAnnotation((*name).to_string()),
                binding.range,
            ));
        }
    }
}
