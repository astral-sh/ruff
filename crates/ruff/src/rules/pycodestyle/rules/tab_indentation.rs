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

/// `string_lines` is parsed from top to bottom during the tokenization phase, and we know that the
/// strings aren't overlapping (otherwise there'd only be one string). This function performs a
/// binary search on `string_lines` to find the string that contains (or starts just before) lineno
fn find_closest_string<'a>(lineno: usize, string_lines: &'a [Range]) -> Option<&'a Range> {
    if string_lines.is_empty() {
        return None;
    }

    let mut low = 0;
    let mut high = string_lines.len() - 1;

    while low <= high {
        let middle = low + (high - low) / 2;
        if middle == 0 {
            break;
        }

        let curr = &string_lines[middle];
        let start = curr.location.row();
        let end = curr.end_location.row();

        if start <= lineno && lineno <= end {
            return Some(curr);
        } else if start > lineno {
            high = middle - 1;
        } else if end < lineno {
            low = middle + 1;
        }
    }

    Some(&string_lines[high])
}

/// W191
pub fn tab_indentation(lineno: usize, line: &str, string_lines: &[Range]) -> Option<Diagnostic> {
    let indent = leading_space(line);

    if indent.contains('\t') {
        // If the tab character is contained in a string, don't raise a violation
        if let Some(contained_range) = find_closest_string(lineno, string_lines) {
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

    fn get_string_lines() -> Vec<Range> {
        vec![
            Range::new(Location::new(1, 0), Location::new(3, 0)),
            Range::new(Location::new(5, 0), Location::new(5, 0)),
            Range::new(Location::new(8, 0), Location::new(10, 0)),
        ]
    }

    #[test]
    // string contains lineno - returns string range with line
    fn test_find_closest_string_contains() {
        let string_lines = get_string_lines();

        let expected = Some(&string_lines[0]);
        let actual = find_closest_string(2usize, &string_lines);
        assert_eq!(expected, actual);

        let expected = Some(&string_lines[0]);
        let actual = find_closest_string(3usize, &string_lines);
        assert_eq!(expected, actual);
    }

    #[test]
    // string doesn't contain lineno - returns closest string range
    fn test_find_closest_string_found() {
        let string_lines = get_string_lines();

        let expected = Some(&string_lines[1]);
        let actual = find_closest_string(6usize, &string_lines);
        assert_eq!(expected, actual);

        let expected = Some(&string_lines[2]);
        let actual = find_closest_string(11usize, &string_lines);
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_find_closest_string_empty_array() {
        assert_eq!(None, find_closest_string(1usize, &[]));
    }
}
