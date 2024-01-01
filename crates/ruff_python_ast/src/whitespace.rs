use ruff_python_trivia::{indentation_at_offset, is_python_whitespace, PythonWhitespace};
use ruff_source_file::{Locator, UniversalNewlineIterator};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Stmt;

/// Extract the leading indentation from a line.
#[inline]
pub fn indentation<'a, T>(locator: &'a Locator, located: &T) -> Option<&'a str>
where
    T: Ranged,
{
    indentation_at_offset(located.start(), locator)
}

/// Return the end offset at which the empty lines following a statement.
pub fn trailing_lines_end(stmt: &Stmt, locator: &Locator) -> TextSize {
    let line_end = locator.full_line_end(stmt.end());
    UniversalNewlineIterator::with_offset(locator.after(line_end), line_end)
        .take_while(|line| line.trim_whitespace().is_empty())
        .last()
        .map_or(line_end, |line| line.full_end())
}

/// If a [`Ranged`] has a trailing comment, return the index of the hash.
pub fn trailing_comment_start_offset<T>(located: &T, locator: &Locator) -> Option<TextSize>
where
    T: Ranged,
{
    let line_end = locator.line_end(located.end());

    let trailing = locator.slice(TextRange::new(located.end(), line_end));

    for (index, char) in trailing.char_indices() {
        if char == '#' {
            return TextSize::try_from(index).ok();
        }
        if !is_python_whitespace(char) {
            return None;
        }
    }

    None
}
