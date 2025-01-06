use ruff_source_file::LineRanges;
use ruff_text_size::{TextRange, TextSize};

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
