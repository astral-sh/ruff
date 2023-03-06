use ruff_macros::{derive_message_formats, violation};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::ast::whitespace::leading_space;
use crate::registry::Diagnostic;
use crate::violation::Violation;

#[violation]
pub struct IndentationContainsTabs;

impl Violation for IndentationContainsTabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains tabs")
    }
}

/// W191
pub fn indentation_contains_tabs(lineno: usize, line: &str) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains('\t') {
        Some(Diagnostic::new(
            IndentationContainsTabs,
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}
