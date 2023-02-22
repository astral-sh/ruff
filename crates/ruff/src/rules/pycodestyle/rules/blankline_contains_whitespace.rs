use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct BlanklineContainsWhitespace;
);
impl AlwaysAutofixableViolation for BlanklineContainsWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Blank line contains whitespace")
    }

    fn autofix_title(&self) -> String {
        "Remove whitespaces".to_string()
    }
}

/// W293
pub fn blankline_contains_whitespace(
    lineno: usize,
    line: &str,
    autofix: bool,
) -> Option<Diagnostic> {
    if line.trim().is_empty() {
        let start = Location::new(lineno + 1, 0);
        let end = Location::new(lineno + 1, line.len());
        let mut diagnostic = Diagnostic::new(BlanklineContainsWhitespace, Range::new(start, end));
        if autofix {
            diagnostic.amend(Fix::deletion(start, end));
        }
        Some(diagnostic)
    } else {
        None
    }
}
