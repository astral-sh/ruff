use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::is_ambiguous_name;
use crate::violations;

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
