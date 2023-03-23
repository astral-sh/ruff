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

/// Strip the leading and trailing quotes from a string.
/// Assumes that the string is a valid string literal, but does not verify that the string
/// is a "simple" string literal (i.e., that it does not contain any implicit concatenations).
pub fn raw_contents(contents: &str) -> Option<&str> {
    let leading_quote_str = leading_quote(contents)?;
    let trailing_quote_str = trailing_quote(contents)?;
    Some(&contents[leading_quote_str.len()..contents.len() - trailing_quote_str.len()])
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

/// Return `true` if the string expression is an implicit concatenation.
///
/// ## Examples
///
/// ```rust
/// use ruff_python_ast::str::is_implicit_concatenation;
///
/// assert!(is_implicit_concatenation(r#"'abc' 'def'"#));
/// assert!(!is_implicit_concatenation(r#"'abcdef'"#));
/// ```
pub fn is_implicit_concatenation(content: &str) -> bool {
    let Some(leading_quote_str) = leading_quote(content) else {
        return false;
    };
    let Some(trailing_quote_str) = trailing_quote(content) else {
        return false;
    };

    // If the trailing quote doesn't match the _expected_ trailing quote, then the string is
    // implicitly concatenated.
    //
    // For example, given:
    // ```python
    // u"""abc""" 'def'
    // ```
    //
    // The leading quote would be `u"""`, and the trailing quote would be `'`, but the _expected_
    // trailing quote would be `"""`. Since `'` does not equal `"""`, we'd return `true`.
    if trailing_quote_str != trailing_quote(leading_quote_str).unwrap() {
        return true;
    }

    // Search for any trailing quotes _before_ the end of the string.
    let mut rest = &content[leading_quote_str.len()..content.len() - trailing_quote_str.len()];
    while let Some(index) = rest.find(trailing_quote_str) {
        let mut chars = rest[..index].chars().rev();
        if let Some('\\') = chars.next() {
            // If the quote is double-escaped, then it's _not_ escaped, so the string is
            // implicitly concatenated.
            if let Some('\\') = chars.next() {
                return true;
            }
        } else {
            // If the quote is _not_ escaped, then it's implicitly concatenated.
            return true;
        }
        rest = &rest[index + trailing_quote_str.len()..];
    }

    // Otherwise, we know the string ends with the expected trailing quote, so it's not implicitly
    // concatenated.
    false
}

#[cfg(test)]
mod tests {
    use crate::str::is_implicit_concatenation;

    use super::{
        SINGLE_QUOTE_BYTE_PREFIXES, SINGLE_QUOTE_STR_PREFIXES, TRIPLE_QUOTE_BYTE_PREFIXES,
        TRIPLE_QUOTE_STR_PREFIXES,
    };

    #[test]
    fn prefix_uniqueness() {
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

    #[test]
    fn implicit_concatenation() {
        // Positive cases.
        assert!(is_implicit_concatenation(r#""abc" "def""#));
        assert!(is_implicit_concatenation(r#""abc" 'def'"#));
        assert!(is_implicit_concatenation(r#""""abc""" "def""#));
        assert!(is_implicit_concatenation(r#"'''abc''' 'def'"#));
        assert!(is_implicit_concatenation(r#""""abc""" 'def'"#));
        assert!(is_implicit_concatenation(r#"'''abc''' "def""#));
        assert!(is_implicit_concatenation(r#""""abc""""def""#));
        assert!(is_implicit_concatenation(r#"'''abc''''def'"#));
        assert!(is_implicit_concatenation(r#""""abc"""'def'"#));
        assert!(is_implicit_concatenation(r#"'''abc'''"def""#));

        // Negative cases.
        assert!(!is_implicit_concatenation(r#""abc""#));
        assert!(!is_implicit_concatenation(r#"'abc'"#));
        assert!(!is_implicit_concatenation(r#""""abc""""#));
        assert!(!is_implicit_concatenation(r#"'''abc'''"#));
        assert!(!is_implicit_concatenation(r#""""ab"c""""#));
        assert!(!is_implicit_concatenation(r#"'''ab'c'''"#));
        assert!(!is_implicit_concatenation(r#""""ab'c""""#));
        assert!(!is_implicit_concatenation(r#"'''ab"c'''"#));
        assert!(!is_implicit_concatenation(r#""""ab'''c""""#));
        assert!(!is_implicit_concatenation(r#"'''ab"""c'''"#));

        // Positive cases with escaped quotes.
        assert!(is_implicit_concatenation(r#""abc\\""def""#));
        assert!(is_implicit_concatenation(r#""abc\\""def""#));

        // Negative cases with escaped quotes.
        assert!(!is_implicit_concatenation(r#""abc\"def""#));
    }
}
