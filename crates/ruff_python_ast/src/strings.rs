/// Strip the leading and trailing quotes from a docstring.
pub fn raw_contents(contents: &str) -> &str {
    for pattern in ruff_python_stdlib::str::TRIPLE_QUOTE_PREFIXES
        .iter()
        .chain(ruff_python_stdlib::bytes::TRIPLE_QUOTE_PREFIXES)
    {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 3];
        }
    }
    for pattern in ruff_python_stdlib::str::SINGLE_QUOTE_PREFIXES
        .iter()
        .chain(ruff_python_stdlib::bytes::SINGLE_QUOTE_PREFIXES)
    {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 1];
        }
    }
    unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
}

/// Return the leading quote for a string or byte literal (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    if let Some(first_line) = content.lines().next() {
        for pattern in ruff_python_stdlib::str::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(ruff_python_stdlib::bytes::TRIPLE_QUOTE_PREFIXES)
            .chain(ruff_python_stdlib::str::SINGLE_QUOTE_PREFIXES)
            .chain(ruff_python_stdlib::bytes::SINGLE_QUOTE_PREFIXES)
        {
            if first_line.starts_with(pattern) {
                return Some(pattern);
            }
        }
    }
    None
}

/// Return the trailing quote string for a string or byte literal (e.g., `"""`).
pub fn trailing_quote(content: &str) -> Option<&&str> {
    ruff_python_stdlib::str::TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(ruff_python_stdlib::str::SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_prefixes() {
        let prefixes = ruff_python_stdlib::str::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(ruff_python_stdlib::bytes::TRIPLE_QUOTE_PREFIXES)
            .chain(ruff_python_stdlib::str::SINGLE_QUOTE_PREFIXES)
            .chain(ruff_python_stdlib::bytes::SINGLE_QUOTE_PREFIXES)
            .collect::<Vec<_>>();
        for (i, prefix_i) in prefixes.iter().enumerate() {
            for (j, prefix_j) in prefixes.iter().enumerate() {
                if i > j {
                    assert!(
                        !prefix_i.starts_with(*prefix_j),
                        "Prefixes are not unique: {prefix_i} starts with {prefix_j}",
                    );
                }
            }
        }
    }
}
