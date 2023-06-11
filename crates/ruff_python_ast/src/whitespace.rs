use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::ast::Ranged;

use ruff_python_whitespace::is_python_whitespace;

use crate::source_code::Locator;

/// Extract the leading indentation from a line.
#[inline]
pub fn indentation<'a, T>(locator: &'a Locator, located: &T) -> Option<&'a str>
where
    T: Ranged,
{
    indentation_at_offset(locator, located.start())
}

/// Extract the leading indentation from a line.
pub fn indentation_at_offset<'a>(locator: &'a Locator, offset: TextSize) -> Option<&'a str> {
    let line_start = locator.line_start(offset);
    let indentation = &locator.contents()[TextRange::new(line_start, offset)];

    if indentation.chars().all(is_python_whitespace) {
        Some(indentation)
    } else {
        None
    }
}
