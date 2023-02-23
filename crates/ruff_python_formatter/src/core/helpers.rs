use ruff_python::string::{
    SINGLE_QUOTE_PREFIXES, SINGLE_QUOTE_SUFFIXES, TRIPLE_QUOTE_PREFIXES, TRIPLE_QUOTE_SUFFIXES,
};

/// Return the leading quote string for a docstring (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    if let Some(first_line) = content.lines().next() {
        for pattern in TRIPLE_QUOTE_PREFIXES.iter().chain(SINGLE_QUOTE_PREFIXES) {
            if first_line.starts_with(pattern) {
                return Some(pattern);
            }
        }
    }
    None
}

/// Return the trailing quote string for a docstring (e.g., `"""`).
pub fn trailing_quote(content: &str) -> Option<&&str> {
    TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}
