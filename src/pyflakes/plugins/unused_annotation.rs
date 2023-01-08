use crate::ast::types::BindingKind;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// F842
pub fn unused_annotation(xxxxxxxx: &mut xxxxxxxx, scope: usize) {
    let scope = &xxxxxxxx.scopes[scope];
    for (name, binding) in scope
        .values
        .iter()
        .map(|(name, index)| (name, &xxxxxxxx.bindings[*index]))
    {
        if binding.used.is_none()
            && matches!(binding.kind, BindingKind::Annotation)
            && !xxxxxxxx.settings.dummy_variable_rgx.is_match(name)
        {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::UnusedAnnotation((*name).to_string()),
                binding.range,
            ));
        }
    }
}
