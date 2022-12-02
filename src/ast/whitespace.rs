use std::str::Lines;

use rustpython_ast::{Located, Location};

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

/// Extract the leading indentation from a line.
pub fn indentation<T>(checker: &Checker, located: &Located<T>) -> String {
    let range = Range::from_located(located);
    checker
        .locator
        .slice_source_code_range(&Range {
            location: Location::new(range.location.row(), 0),
            end_location: Location::new(range.location.row(), range.location.column()),
        })
        .to_string()
}

/// Replace any non-whitespace characters from an indentation string.
pub fn clean(indentation: &str) -> String {
    indentation
        .chars()
        .map(|char| if char.is_whitespace() { char } else { ' ' })
        .collect()
}

/// Like `str#lines`, but includes a trailing newline as an empty line.
pub struct LinesWithTrailingNewline<'a> {
    trailing: Option<&'a str>,
    underlying: Lines<'a>,
}

impl<'a> LinesWithTrailingNewline<'a> {
    pub fn from(input: &'a str) -> LinesWithTrailingNewline<'a> {
        LinesWithTrailingNewline {
            underlying: input.lines(),
            trailing: if input.ends_with('\n') {
                Some("")
            } else {
                None
            },
        }
    }
}

impl<'a> Iterator for LinesWithTrailingNewline<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        let mut next = self.underlying.next();
        if next.is_none() {
            if self.trailing.is_some() {
                next = self.trailing;
                self.trailing = None;
            }
        }
        next
    }
}
