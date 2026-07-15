use std::borrow::Cow;

use ruff_source_file::LineRanges;
use ruff_text_size::{TextRange, TextSize};

/// Expands tabs to the next eight-column tab stop, matching Python's `str.expandtabs`.
pub fn expand_tabs(source: &str) -> Cow<'_, str> {
    const TAB_SIZE: usize = 8;

    if !source.contains('\t') {
        return Cow::Borrowed(source);
    }

    let mut expanded = String::with_capacity(source.len());
    let mut column = 0;

    for character in source.chars() {
        match character {
            '\t' => {
                let spaces = tab_offset(column, TAB_SIZE);
                expanded.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\r' | '\n' => {
                expanded.push(character);
                column = 0;
            }
            _ => {
                expanded.push(character);
                column += 1;
            }
        }
    }

    Cow::Owned(expanded)
}

/// Returns the number of columns from `column` to the next tab stop.
pub const fn tab_offset(column: usize, tab_size: usize) -> usize {
    tab_size - column % tab_size
}

/// Returns the number of columns from `column` to the next tab stop using `u32` values.
pub const fn tab_offset_u32(column: u32, tab_size: u32) -> u32 {
    tab_size - column % tab_size
}

/// Extract the leading indentation from a line.
pub fn indentation_at_offset(offset: TextSize, source: &str) -> Option<&str> {
    let line_start = source.line_start(offset);
    let indentation = &source[TextRange::new(line_start, offset)];

    indentation
        .chars()
        .all(is_python_whitespace)
        .then_some(indentation)
}

/// Return `true` if the node starting the given [`TextSize`] has leading content.
pub fn has_leading_content(offset: TextSize, source: &str) -> bool {
    let line_start = source.line_start(offset);
    let leading = &source[TextRange::new(line_start, offset)];
    leading.chars().any(|char| !is_python_whitespace(char))
}

/// Return `true` if the node ending at the given [`TextSize`] has trailing content.
pub fn has_trailing_content(offset: TextSize, source: &str) -> bool {
    let line_end = source.line_end(offset);
    let trailing = &source[TextRange::new(offset, line_end)];

    for char in trailing.chars() {
        if char == '#' {
            return false;
        }
        if !is_python_whitespace(char) {
            return true;
        }
    }
    false
}

/// Returns `true` for [whitespace](https://docs.python.org/3/reference/lexical_analysis.html#whitespace-between-tokens)
/// characters.
pub const fn is_python_whitespace(c: char) -> bool {
    matches!(
        c,
        // Space, tab, or form-feed
        ' ' | '\t' | '\x0C'
    )
}

/// Extract the leading indentation from a line.
pub fn leading_indentation(line: &str) -> &str {
    line.find(|char: char| !is_python_whitespace(char))
        .map_or(line, |index| &line[..index])
}

pub trait PythonWhitespace {
    /// Like `str::trim()`, but only removes whitespace characters that Python considers
    /// to be [whitespace](https://docs.python.org/3/reference/lexical_analysis.html#whitespace-between-tokens).
    fn trim_whitespace(&self) -> &Self;

    /// Like `str::trim_start()`, but only removes whitespace characters that Python considers
    /// to be [whitespace](https://docs.python.org/3/reference/lexical_analysis.html#whitespace-between-tokens).
    fn trim_whitespace_start(&self) -> &Self;

    /// Like `str::trim_end()`, but only removes whitespace characters that Python considers
    /// to be [whitespace](https://docs.python.org/3/reference/lexical_analysis.html#whitespace-between-tokens).
    fn trim_whitespace_end(&self) -> &Self;
}

impl PythonWhitespace for str {
    fn trim_whitespace(&self) -> &Self {
        self.trim_matches(is_python_whitespace)
    }

    fn trim_whitespace_start(&self) -> &Self {
        self.trim_start_matches(is_python_whitespace)
    }

    fn trim_whitespace_end(&self) -> &Self {
        self.trim_end_matches(is_python_whitespace)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{expand_tabs, tab_offset, tab_offset_u32};

    #[test]
    fn tab_expansion_borrows_unchanged_text() {
        assert!(matches!(expand_tabs("unchanged"), Cow::Borrowed(_)));
    }

    #[test]
    fn tab_expansion_allocates_changed_text() {
        let expanded = expand_tabs("  \tvalue");

        assert!(matches!(&expanded, Cow::Owned(_)));
        assert_eq!(expanded, "        value");
    }

    #[test]
    fn tab_offset_advances_to_next_stop() {
        assert_eq!(tab_offset(0, 8), 8);
        assert_eq!(tab_offset(2, 8), 6);
        assert_eq!(tab_offset(8, 8), 8);
    }

    #[test]
    fn u32_tab_offset_advances_to_next_stop() {
        assert_eq!(tab_offset_u32(0, 8), 8);
        assert_eq!(tab_offset_u32(2, 8), 6);
        assert_eq!(tab_offset_u32(8, 8), 8);
    }
}
