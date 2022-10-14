use rustpython_ast::{Expr, Location};

use crate::ast::types::Range;
use crate::check_ast::Checker;

/// Extract the leading words from a line of text.
pub fn leading_words(line: &str) -> String {
    line.trim()
        .chars()
        .take_while(|char| char.is_alphanumeric() || char.is_whitespace())
        .collect()
}

/// Extract the leading whitespace from a line of text.
pub fn leading_space(line: &str) -> String {
    line.chars()
        .take_while(|char| char.is_whitespace())
        .collect()
}

/// Extract the leading indentation from a docstring.
pub fn indentation<'a>(checker: &'a mut Checker, docstring: &Expr) -> &'a str {
    let range = range_for(docstring);
    checker.locator.slice_source_code_range(&Range {
        location: Location::new(range.location.row(), 1),
        end_location: Location::new(range.location.row(), range.location.column()),
    })
}

/// Extract the source code range for a docstring.
pub fn range_for(docstring: &Expr) -> Range {
    // RustPython currently omits the first quotation mark in a string, so offset the location.
    Range {
        location: Location::new(docstring.location.row(), docstring.location.column() - 1),
        end_location: docstring.end_location,
    }
}
