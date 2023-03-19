use std::fmt::{Display, Formatter};
use std::ops::Deref;

/// Strip the leading and trailing quotes from a docstring.
pub fn strip_quotes(contents: &str) -> &str {
    let leading = LeadingQuote::try_from_str(contents)
        .expect("Expected docstring to start with a valid triple-or single-quote prefix");

    &contents[leading.len()..contents.len() - leading.kind().len()]
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum QuoteKind {
    Single,
    Triple,
}

impl QuoteKind {
    fn try_from<I>(content: I) -> Option<QuoteKind>
    where
        I: IntoIterator<Item = u8>,
    {
        let mut bytes = content.into_iter();

        match bytes.next()? {
            b'\'' => match (bytes.next(), bytes.next()) {
                (Some(b'\''), Some(b'\'')) => Some(QuoteKind::Triple),
                _ => Some(QuoteKind::Single),
            },
            b'"' => match (bytes.next(), bytes.next()) {
                (Some(b'"'), Some(b'"')) => Some(QuoteKind::Triple),
                _ => Some(QuoteKind::Single),
            },
            _ => None,
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        match self {
            QuoteKind::Single => 1,
            QuoteKind::Triple => 3,
        }
    }

    pub const fn is_triple(&self) -> bool {
        matches!(self, QuoteKind::Triple)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LeadingQuote<'a> {
    content: &'a str,
}

impl<'a> LeadingQuote<'a> {
    /// Return the leading quote for a string or byte literal (e.g., `"""`).
    pub fn try_from_str(content: &'a str) -> Option<Self> {
        let mut bytes = content.bytes();

        let prefix_len = match bytes.next()? {
            b'\'' | b'"' => 0,
            b'b' | b'B' => match bytes.next()? {
                b'r' | b'R' => 2,
                _ => 1,
            },
            b'r' | b'R' => match bytes.next()? {
                b'b' | b'B' => 2,
                _ => 1,
            },
            b'u' | b'U' => 1,
            _ => return None,
        };

        let kind = QuoteKind::try_from(content[prefix_len..].bytes())?;

        Some(LeadingQuote {
            content: &content[..prefix_len + kind.len()],
        })
    }

    pub fn kind(&self) -> QuoteKind {
        let mut iter = self.content.bytes().rev();

        match (iter.next(), iter.next()) {
            (Some(b'\'' | b'"'), Some(b'\'' | b'"')) => QuoteKind::Triple,
            _ => QuoteKind::Single,
        }
    }

    pub fn trailing_quote(&self) -> TrailingQuote<'a> {
        TrailingQuote {
            content: &self.content[self.content.len() - self.kind().len()..],
        }
    }

    pub const fn as_str(&self) -> &'a str {
        self.content
    }
}

impl Deref for LeadingQuote<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Display for LeadingQuote<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.content)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct TrailingQuote<'a> {
    content: &'a str,
}

impl<'a> TrailingQuote<'a> {
    /// Return the trailing quote string for a string or byte literal (e.g., `"""`).
    pub fn try_from_str(content: &'a str) -> Option<Self> {
        let kind = QuoteKind::try_from(content.bytes().rev())?;

        Some(Self {
            content: &content[content.len() - kind.len()..],
        })
    }

    pub const fn kind(&self) -> QuoteKind {
        match self.content.len() {
            1 => QuoteKind::Single,
            _ => QuoteKind::Triple,
        }
    }

    pub fn as_str(&self) -> &'a str {
        self.content
    }
}

impl Display for TrailingQuote<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.content)
    }
}

impl Deref for TrailingQuote<'_> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.content
    }
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
    let Some(leading) = LeadingQuote::try_from_str(content) else {
        return false;
    };
    let Some(trailing) = TrailingQuote::try_from_str(content) else {
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
    if trailing != leading.trailing_quote() {
        return true;
    }

    // Search for any trailing quotes _before_ the end of the string.
    let mut rest = &content[leading.len()..content.len() - trailing.len()];
    while let Some(index) = rest.find(trailing.as_str()) {
        let mut chars = rest[..index].bytes().rev();
        if let Some(b'\\') = chars.next() {
            // If the quote is double-escaped, then it's _not_ escaped, so the string is
            // implicitly concatenated.
            if let Some(b'\\') = chars.next() {
                return true;
            }
        } else {
            // If the quote is _not_ escaped, then it's implicitly concatenated.
            return true;
        }
        rest = &rest[index + trailing.len()..];
    }

    // Otherwise, we know the string ends with the expected trailing quote, so it's not implicitly
    // concatenated.
    false
}

#[cfg(test)]
mod tests {
    use crate::str::{is_implicit_concatenation, LeadingQuote, QuoteKind, TrailingQuote};

    /// See: <https://docs.python.org/3/reference/lexical_analysis.html#string-and-bytes-literals>
    const TRIPLE_QUOTE_STR_PREFIXES: &[&str] = &[
        "br'''", "rb'''", "bR'''", "Rb'''", "Br'''", "rB'''", "RB'''", "BR'''", "b'''", "br\"\"\"",
        "rb\"\"\"", "bR\"\"\"", "Rb\"\"\"", "Br\"\"\"", "rB\"\"\"", "RB\"\"\"", "BR\"\"\"",
        "b\"\"\"", "B\"\"\"", "u\"\"\"", "u'''", "r\"\"\"", "r'''", "U\"\"\"", "U'''", "R\"\"\"",
        "R'''", "\"\"\"", "'''",
    ];
    const SINGLE_QUOTE_STR_PREFIXES: &[&str] = &[
        "br'", "rb'", "bR'", "Rb'", "Br'", "rB'", "RB'", "BR'", "b'", "br\"", "rb\"", "bR\"",
        "Rb\"", "Br\"", "rB\"", "RB\"", "BR\"", "b\"", "B\"", "u\"", "u'", "r\"", "r'", "U\"",
        "U'", "R\"", "R'", "\"", "'",
    ];
    const TRIPLE_QUOTE_SUFFIXES: &[&str] = &["\"\"\"", "'''"];
    const SINGLE_QUOTE_SUFFIXES: &[&str] = &["\"", "'"];

    #[test]
    fn single_quote_prefixes() {
        for quote in SINGLE_QUOTE_STR_PREFIXES {
            let leading = LeadingQuote::try_from_str(quote).unwrap();

            assert_eq!(&leading, &LeadingQuote { content: quote });
            assert_eq!(leading.kind(), QuoteKind::Single);
        }
    }

    #[test]
    fn tripe_quote_prefixes() {
        for quote in TRIPLE_QUOTE_STR_PREFIXES {
            let leading = LeadingQuote::try_from_str(quote).unwrap();
            assert_eq!(leading, LeadingQuote { content: quote });
            assert_eq!(leading.kind(), QuoteKind::Triple);
        }
    }

    #[test]
    fn single_quote_suffixes() {
        for quote in SINGLE_QUOTE_SUFFIXES {
            let trailing = TrailingQuote::try_from_str(quote).unwrap();
            assert_eq!(trailing, TrailingQuote { content: quote });
            assert_eq!(trailing.kind(), QuoteKind::Single);
        }
    }

    #[test]
    fn triple_quote_suffixes() {
        for quote in TRIPLE_QUOTE_SUFFIXES {
            let trailing = TrailingQuote::try_from_str(quote).unwrap();
            assert_eq!(trailing, TrailingQuote { content: quote });
            assert_eq!(trailing.kind(), QuoteKind::Triple);
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
