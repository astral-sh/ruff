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
