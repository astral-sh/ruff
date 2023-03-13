use rustpython_parser::ast::{Located, Location};

use crate::source_code::Locator;
use crate::types::Range;

/// Extract the leading indentation from a line.
pub fn indentation<'a, T>(locator: &'a Locator, located: &'a Located<T>) -> Option<&'a str> {
    let range = Range::from(located);
    let indentation = locator.slice(Range::new(
        Location::new(range.location.row(), 0),
        Location::new(range.location.row(), range.location.column()),
    ));
    if indentation.chars().all(char::is_whitespace) {
        Some(indentation)
    } else {
        None
    }
}

/// Extract the leading words from a line of text.
pub fn leading_words(line: &str) -> &str {
    let line = line.trim();
    line.find(|char: char| !char.is_alphanumeric() && !char.is_whitespace())
        .map_or(line, |index| &line[..index])
}

/// Extract the leading whitespace from a line of text.
pub fn leading_space(line: &str) -> &str {
    line.find(|char: char| !char.is_whitespace())
        .map_or(line, |index| &line[..index])
}

/// Replace any non-whitespace characters from an indentation string.
pub fn clean(indentation: &str) -> String {
    indentation
        .chars()
        .map(|char| if char.is_whitespace() { char } else { ' ' })
        .collect()
}
