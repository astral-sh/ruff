use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

/// E741
pub fn ambiguous_variable_name(name: &str, range: Range) -> Option<Diagnostic> {
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            violations::AmbiguousVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}

/// E742
pub fn ambiguous_class_name<F>(name: &str, locate: F) -> Option<Diagnostic>
where
    F: FnOnce() -> Range,
{
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            violations::AmbiguousClassName(name.to_string()),
            locate(),
        ))
    } else {
        None
    }
}

/// E743
pub fn ambiguous_function_name<F>(name: &str, locate: F) -> Option<Diagnostic>
where
    F: FnOnce() -> Range,
{
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            violations::AmbiguousFunctionName(name.to_string()),
            locate(),
        ))
    } else {
        None
    }
}
