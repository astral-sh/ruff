use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct BlanklineContainsWhitespace;
);
impl Violation for BlanklineContainsWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Blank line contains whitespace")
    }
}

/// W293
pub fn blankline_contains_whitespace(lineno: usize, line: &str) -> Option<Diagnostic> {
    if line.trim().is_empty() {
        Some(Diagnostic::new(
            BlanklineContainsWhitespace,
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, line.len()),
            ),
        ))
    } else {
        None
    }
}
