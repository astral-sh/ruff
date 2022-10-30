use rustpython_ast::{Expr, Location};

use crate::ast::types::Range;
use crate::check_ast::Checker;

pub const TRIPLE_QUOTE_PREFIXES: &[&str] = &[
    "ur\"\"\"", "ur'''", "u\"\"\"", "u'''", "r\"\"\"", "r'''", "\"\"\"", "'''",
];

pub const SINGLE_QUOTE_PREFIXES: &[&str] = &["ur\"", "ur'", "u\"", "u'", "r\"", "r'", "\"", "'"];

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
pub fn indentation<'a>(checker: &'a Checker, docstring: &Expr) -> &'a str {
    let range = Range::from_located(docstring);
    checker.locator.slice_source_code_range(&Range {
        location: Location::new(range.location.row(), 0),
        end_location: Location::new(range.location.row(), range.location.column()),
    })
}

/// Replace any non-whitespace characters from an indentation string.
pub fn clean(indentation: &str) -> String {
    indentation
        .chars()
        .map(|char| if char.is_whitespace() { char } else { ' ' })
        .collect()
}
