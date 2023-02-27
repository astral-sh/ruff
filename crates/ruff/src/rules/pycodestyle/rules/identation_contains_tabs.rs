use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct IdentationContainsTabs;
);
impl Violation for IdentationContainsTabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains tabs")
    }
}

/// W191
pub fn identation_contains_tabs(lineno: usize, line: &str) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains('\t') {
        Some(Diagnostic::new(
            IdentationContainsTabs,
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}
