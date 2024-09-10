//! Utilities for parsing Python docstrings.

/// Extract the leading words from a line of text within a Python docstring.
pub fn leading_words(line: &str) -> &str {
    let line = line.trim();
    line.find(|char: char| !char.is_alphanumeric() && !char.is_whitespace())
        .map_or(line, |index| &line[..index])
}

/// Extract the leading whitespace from a line of text within a Python docstring.
pub fn leading_space(line: &str) -> &str {
    line.find(|char: char| !char.is_whitespace())
        .map_or(line, |index| &line[..index])
}

/// Replace any non-whitespace characters from an indentation string within a Python docstring.
pub fn clean_space(indentation: &str) -> String {
    indentation
        .chars()
        .map(|char| if char.is_whitespace() { char } else { ' ' })
        .collect()
}

/// Extract the leading whitespace and colon from a line of text within a Python docstring.
pub fn leading_space_and_colon(line: &str) -> &str {
    line.find(|char: char| !char.is_whitespace() && char != ':')
        .map_or(line, |index| &line[..index])
}

/// Sphinx section section name.
pub fn sphinx_section_name(line: &str) -> Option<&str> {
    let mut spans = line.split(':');
    let _indentation = spans.next()?;
    let header = spans.next()?;
    let _after_header = spans.next()?;
    header.split(' ').next()
}
