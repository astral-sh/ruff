use ruff_python_stdlib::str::{
    SINGLE_QUOTE_PREFIXES, SINGLE_QUOTE_SUFFIXES, TRIPLE_QUOTE_PREFIXES, TRIPLE_QUOTE_SUFFIXES,
};

/// Strip the leading and trailing quotes from a docstring.
pub fn raw_contents(contents: &str) -> &str {
    for pattern in TRIPLE_QUOTE_PREFIXES {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 3];
        }
    }
    for pattern in SINGLE_QUOTE_PREFIXES {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 1];
        }
    }
    unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
}

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
