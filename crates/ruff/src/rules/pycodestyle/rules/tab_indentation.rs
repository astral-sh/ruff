use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::whitespace::leading_space;

#[violation]
pub struct TabIndentation;

impl Violation for TabIndentation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Indentation contains tabs")
    }
}

fn find_closest_string(lineno: &usize, string_lines: &[(usize, usize)]) -> Option<(usize, usize)> {
    if string_lines.is_empty() {
        return None;
    }

    Some((0usize, 1usize))

    // we know that string_lines is an ordered list since the file is parsed top to bottom, - we can
    // therefore binary search the list to find the start of the closest string, then check if
    // this line is in that range
    // let mut l = 0usize;
    // let mut h = string_lines.len(); // not using len() - 1 so we can avoid underflow errors
    // let mut m = 0usize;

    // while l <= h {
    //     m = l + (h - l) / 2;

    //     let (m_start, m_end) = string_lines[m];
    //     if m_start > *lineno {
    //         h = m;
    //     } else if m_end < *lineno {
    //         l = m + 1;
    //     }
    // }

    // Some(string_lines[m])
}

/// W191
pub fn tab_indentation(lineno: usize, line: &str, string_lines: &[Range]) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains('\t') {
        // If the tab character is contained in a string, don't raise a violation
        if let Some(contained_range) = find_closest_string(&lineno, string_lines) {
            if contained_range.location.row() <= lineno
                && contained_range.end_location.row() >= lineno
            {
                return None;
            }
        }

        Some(Diagnostic::new(
            TabIndentation,
            Range::new(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 1, indent.chars().count()),
            ),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    // string contains lineno - don't throw
    fn test_find_closest_string_contains() {}

    #[test]
    // string doesn't contain lineno - throw
    fn test_find_closest_string_found() {}

    #[test]
    // no strings
    fn test_find_closest_string_none() {
        assert_eq!(None, find_closest_string(&1usize, &[]));
    }
}
