use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::docstrings::constants;
use crate::SourceCodeLocator;

/// Strip the leading and trailing quotes from a docstring.
pub fn raw_contents(contents: &str) -> &str {
    for pattern in constants::TRIPLE_QUOTE_PREFIXES {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 3];
        }
    }
    for pattern in constants::SINGLE_QUOTE_PREFIXES {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 1];
        }
    }
    unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
}

/// Return the leading quote string for a docstring (e.g., `"""`).
pub fn leading_quote<'a>(docstring: &Expr, locator: &'a SourceCodeLocator) -> Option<&'a str> {
    if let Some(first_line) = locator
        .slice_source_code_range(&Range::from_located(docstring))
        .lines()
        .next()
    {
        for pattern in constants::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(constants::SINGLE_QUOTE_PREFIXES)
        {
            if first_line.starts_with(pattern) {
                return Some(pattern);
            }
        }
    }
    None
}

/// Return the index of the first logical line in a string.
pub fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            // Empty line. If this is the line _after_ the first logical line, stop.
            if logical_line.is_some() {
                break;
            }
        } else {
            // Non-empty line. Store the index.
            logical_line = Some(i);
        }
    }
    logical_line
}
