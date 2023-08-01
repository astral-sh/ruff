use crate::{Ranged, Stmt};
use ruff_text_size::{TextRange, TextSize};

use ruff_python_trivia::{
    has_trailing_content, indentation_at_offset, is_python_whitespace, PythonWhitespace,
};
use ruff_source_file::{newlines::UniversalNewlineIterator, Locator};

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
    let rest = &locator.contents()[usize::from(line_end)..];

    UniversalNewlineIterator::with_offset(rest, line_end)
        .take_while(|line| line.trim_whitespace().is_empty())
        .last()
        .map_or(line_end, |line| line.full_end())
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements following it.
pub fn followed_by_multi_statement_line(stmt: &Stmt, locator: &Locator) -> bool {
    has_trailing_content(stmt.end(), locator)
}

/// If a [`Ranged`] has a trailing comment, return the index of the hash.
pub fn trailing_comment_start_offset<T>(located: &T, locator: &Locator) -> Option<TextSize>
where
    T: Ranged,
{
    let line_end = locator.line_end(located.end());

    let trailing = &locator.contents()[TextRange::new(located.end(), line_end)];

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
