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
