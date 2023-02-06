use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::rules::pycodestyle::helpers::is_ambiguous_name;
use crate::violation::Violation;

define_violation!(
    pub struct AmbiguousVariableName(pub String);
);
impl Violation for AmbiguousVariableName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousVariableName(name) = self;
        format!("Ambiguous variable name: `{name}`")
    }
}

/// E741
pub fn ambiguous_variable_name(name: &str, range: Range) -> Option<Diagnostic> {
    if is_ambiguous_name(name) {
        Some(Diagnostic::new(
            AmbiguousVariableName(name.to_string()),
            range,
        ))
    } else {
        None
    }
}
