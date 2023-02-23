/// Return the leading quote for a string or byte literal (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    if let Some(first_line) = content.lines().next() {
        for pattern in ruff_python::str::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(ruff_python::bytes::TRIPLE_QUOTE_PREFIXES)
            .chain(ruff_python::str::SINGLE_QUOTE_PREFIXES)
            .chain(ruff_python::bytes::SINGLE_QUOTE_PREFIXES)
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
    ruff_python::str::TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(ruff_python::str::SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_prefixes() {
        let prefixes = ruff_python::str::TRIPLE_QUOTE_PREFIXES
            .iter()
            .chain(ruff_python::bytes::TRIPLE_QUOTE_PREFIXES)
            .chain(ruff_python::str::SINGLE_QUOTE_PREFIXES)
            .chain(ruff_python::bytes::SINGLE_QUOTE_PREFIXES)
            .collect::<Vec<_>>();
        for i in 1..prefixes.len() {
            for j in 0..i - 1 {
                if i != j {
                    if prefixes[i].starts_with(prefixes[j]) {
                        assert!(
                            !prefixes[i].starts_with(prefixes[j]),
                            "Prefixes are not unique: {} starts with {}",
                            prefixes[i],
                            prefixes[j]
                        );
                    }
                }
            }
        }
    }
}
