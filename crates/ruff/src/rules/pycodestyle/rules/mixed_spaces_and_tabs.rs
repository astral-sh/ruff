use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct MixedSpacesAndTabs;
);
impl Violation for MixedSpacesAndTabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains mixed spaces and tabs")
    }
}

/// E101
pub fn mixed_spaces_and_tabs(lineno: usize, line: &str) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains(' ') && indent.contains('\t') {
        Some(Diagnostic::new(
            MixedSpacesAndTabs,
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}
