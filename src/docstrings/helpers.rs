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
    let range = Range::from_located(docstring);
    checker.locator.slice_source_code_range(&Range {
        location: Location::new(range.location.row(), 1),
        end_location: Location::new(range.location.row(), range.location.column()),
    })
}
