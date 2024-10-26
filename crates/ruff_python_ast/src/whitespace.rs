use ruff_python_trivia::{indentation_at_offset, is_python_whitespace, PythonWhitespace};
use ruff_source_file::{Located, UniversalNewlineIterator};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Stmt;

/// Extract the leading indentation from a line.
#[inline]
pub fn indentation<'a, T>(source: &'a str, ranged: &T) -> Option<&'a str>
where
    T: Ranged,
{
    indentation_at_offset(ranged.start(), source)
}

/// Return the end offset at which the empty lines following a statement.
pub fn trailing_lines_end(stmt: &Stmt, source: &str) -> TextSize {
    let line_end = source.full_line_end(stmt.end());
    UniversalNewlineIterator::with_offset(source.after(line_end), line_end)
        .take_while(|line| line.trim_whitespace().is_empty())
        .last()
        .map_or(line_end, |line| line.full_end())
}

/// If a [`Ranged`] has a trailing comment, return the index of the hash.
pub fn trailing_comment_start_offset<T>(located: &T, source: &str) -> Option<TextSize>
where
    T: Ranged,
{
    let line_end = source.line_end(located.end());

    let trailing = source.slice(TextRange::new(located.end(), line_end));

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
