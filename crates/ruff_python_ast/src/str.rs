use ruff_text_size::{TextLen, TextRange};

/// Includes all permutations of `r`, `u`, `f`, and `fr` (`ur` is invalid, as is `uf`). This
/// includes all possible orders, and all possible casings, for both single and triple quotes.
///
/// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
#[rustfmt::skip]
const TRIPLE_QUOTE_STR_PREFIXES: &[&str] = &[
    "FR\"\"\"",
    "Fr\"\"\"",
    "fR\"\"\"",
    "fr\"\"\"",
    "RF\"\"\"",
    "Rf\"\"\"",
    "rF\"\"\"",
    "rf\"\"\"",
    "FR'''",
    "Fr'''",
    "fR'''",
    "fr'''",
    "RF'''",
    "Rf'''",
    "rF'''",
    "rf'''",
    "R\"\"\"",
    "r\"\"\"",
    "R'''",
    "r'''",
    "F\"\"\"",
    "f\"\"\"",
    "F'''",
    "f'''",
    "U\"\"\"",
    "u\"\"\"",
    "U'''",
    "u'''",
    "\"\"\"",
    "'''",
];

#[rustfmt::skip]
const SINGLE_QUOTE_STR_PREFIXES: &[&str] = &[
    "FR\"",
    "Fr\"",
    "fR\"",
    "fr\"",
    "RF\"",
    "Rf\"",
    "rF\"",
    "rf\"",
    "FR'",
    "Fr'",
    "fR'",
    "fr'",
    "RF'",
    "Rf'",
    "rF'",
    "rf'",
    "R\"",
    "r\"",
    "R'",
    "r'",
    "F\"",
    "f\"",
    "F'",
    "f'",
    "U\"",
    "u\"",
    "U'",
    "u'",
    "\"",
    "'",
];

/// Includes all permutations of `b` and `rb`. This includes all possible orders, and all possible
/// casings, for both single and triple quotes.
///
/// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
#[rustfmt::skip]
pub const TRIPLE_QUOTE_BYTE_PREFIXES: &[&str] = &[
    "BR\"\"\"",
    "Br\"\"\"",
    "bR\"\"\"",
    "br\"\"\"",
    "RB\"\"\"",
    "Rb\"\"\"",
    "rB\"\"\"",
    "rb\"\"\"",
    "BR'''",
    "Br'''",
    "bR'''",
    "br'''",
    "RB'''",
    "Rb'''",
    "rB'''",
    "rb'''",
    "B\"\"\"",
    "b\"\"\"",
    "B'''",
    "b'''",
];

#[rustfmt::skip]
pub const SINGLE_QUOTE_BYTE_PREFIXES: &[&str] = &[
    "BR\"",
    "Br\"",
    "bR\"",
    "br\"",
    "RB\"",
    "Rb\"",
    "rB\"",
    "rb\"",
    "BR'",
    "Br'",
    "bR'",
    "br'",
    "RB'",
    "Rb'",
    "rB'",
    "rb'",
    "B\"",
    "b\"",
    "B'",
    "b'",
];

#[rustfmt::skip]
const TRIPLE_QUOTE_SUFFIXES: &[&str] = &[
    "\"\"\"",
    "'''",
];

#[rustfmt::skip]
const SINGLE_QUOTE_SUFFIXES: &[&str] = &[
    "\"",
    "'",
];

/// Strip the leading and trailing quotes from a string.
/// Assumes that the string is a valid string literal, but does not verify that the string
/// is a "simple" string literal (i.e., that it does not contain any implicit concatenations).
pub fn raw_contents(contents: &str) -> Option<&str> {
    let range = raw_contents_range(contents)?;

    Some(&contents[range])
}

pub fn raw_contents_range(contents: &str) -> Option<TextRange> {
    let leading_quote_str = leading_quote(contents)?;
    let trailing_quote_str = trailing_quote(contents)?;

    Some(TextRange::new(
        leading_quote_str.text_len(),
        contents.text_len() - trailing_quote_str.text_len(),
    ))
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
}
