/// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
const TRIPLE_QUOTE_STR_PREFIXES: &[&str] = &[
    "u\"\"\"", "u'''", "r\"\"\"", "r'''", "U\"\"\"", "U'''", "R\"\"\"", "R'''", "\"\"\"", "'''",
];
const SINGLE_QUOTE_STR_PREFIXES: &[&str] = &[
    "u\"", "u'", "r\"", "r'", "U\"", "U'", "R\"", "R'", "\"", "'",
];
pub const TRIPLE_QUOTE_BYTE_PREFIXES: &[&str] = &[
    "br'''", "rb'''", "bR'''", "Rb'''", "Br'''", "rB'''", "RB'''", "BR'''", "b'''", "br\"\"\"",
    "rb\"\"\"", "bR\"\"\"", "Rb\"\"\"", "Br\"\"\"", "rB\"\"\"", "RB\"\"\"", "BR\"\"\"", "b\"\"\"",
    "B\"\"\"",
];
pub const SINGLE_QUOTE_BYTE_PREFIXES: &[&str] = &[
    "br'", "rb'", "bR'", "Rb'", "Br'", "rB'", "RB'", "BR'", "b'", "br\"", "rb\"", "bR\"", "Rb\"",
    "Br\"", "rB\"", "RB\"", "BR\"", "b\"", "B\"",
];
const TRIPLE_QUOTE_SUFFIXES: &[&str] = &["\"\"\"", "'''"];
const SINGLE_QUOTE_SUFFIXES: &[&str] = &["\"", "'"];

/// Strip the leading and trailing quotes from a docstring.
pub fn raw_contents(contents: &str) -> &str {
    for pattern in TRIPLE_QUOTE_STR_PREFIXES
        .iter()
        .chain(TRIPLE_QUOTE_BYTE_PREFIXES)
    {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 3];
        }
    }
    for pattern in SINGLE_QUOTE_STR_PREFIXES
        .iter()
        .chain(SINGLE_QUOTE_BYTE_PREFIXES)
    {
        if contents.starts_with(pattern) {
            return &contents[pattern.len()..contents.len() - 1];
        }
    }
    unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
}

/// Return the leading quote for a string or byte literal (e.g., `"""`).
pub fn leading_quote(content: &str) -> Option<&str> {
    TRIPLE_QUOTE_STR_PREFIXES
        .iter()
        .chain(TRIPLE_QUOTE_BYTE_PREFIXES)
        .chain(SINGLE_QUOTE_STR_PREFIXES)
        .chain(SINGLE_QUOTE_BYTE_PREFIXES)
        .find_map(|pattern| {
            if content.starts_with(pattern) {
                Some(*pattern)
            } else {
                None
            }
        })
}

/// Return the trailing quote string for a string or byte literal (e.g., `"""`).
pub fn trailing_quote(content: &str) -> Option<&&str> {
    TRIPLE_QUOTE_SUFFIXES
        .iter()
        .chain(SINGLE_QUOTE_SUFFIXES)
        .find(|&pattern| content.ends_with(pattern))
}

/// Return `true` if the string is a triple-quote string or byte prefix.
pub fn is_triple_quote(content: &str) -> bool {
    TRIPLE_QUOTE_STR_PREFIXES.contains(&content) || TRIPLE_QUOTE_BYTE_PREFIXES.contains(&content)
}

#[cfg(test)]
mod tests {
    use super::{
        SINGLE_QUOTE_BYTE_PREFIXES, SINGLE_QUOTE_STR_PREFIXES, TRIPLE_QUOTE_BYTE_PREFIXES,
        TRIPLE_QUOTE_STR_PREFIXES,
    };

    #[test]
    fn test_prefixes() {
        let prefixes = TRIPLE_QUOTE_STR_PREFIXES
            .iter()
            .chain(TRIPLE_QUOTE_BYTE_PREFIXES)
            .chain(SINGLE_QUOTE_STR_PREFIXES)
            .chain(SINGLE_QUOTE_BYTE_PREFIXES)
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
