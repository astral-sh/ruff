use memchr::memmem::Finder;

/// A reusable text prefilter for an identifier name.
///
/// A match only indicates that a source may contain the name: matches in
/// comments and strings are included, and non-ASCII sources always match because
/// Python normalizes identifiers with NFKC. Callers should validate candidates
/// using the AST or semantic analysis.
///
/// Construct a matcher once and reuse it across sources to avoid preprocessing
/// the name for each source. Cache matchers for fixed names in a [`std::sync::LazyLock`].
#[derive(Debug, Clone)]
pub struct IdentifierMatcher<'a> {
    finder: Finder<'a>,
}

impl<'a> IdentifierMatcher<'a> {
    /// Creates a matcher that borrows `name` without allocating.
    pub fn new(name: &'a str) -> Self {
        Self {
            finder: Finder::new(name),
        }
    }

    /// Returns whether `source` may contain the identifier.
    ///
    /// Returns `true` immediately for non-ASCII source. Otherwise, searches for
    /// the literal spelling bounded by bytes other than ASCII letters, digits or `_`.
    pub fn may_match(&self, source: &str) -> bool {
        if !source.is_ascii() {
            return true;
        }

        let bytes = source.as_bytes();
        let len = self.finder.needle().len();
        self.finder
            .find_iter(bytes)
            .any(|start| has_identifier_boundaries(bytes, start, start + len))
    }
}

fn has_identifier_boundaries(source: &[u8], start: usize, end: usize) -> bool {
    (start == 0 || !is_ascii_identifier_continue(source[start - 1]))
        && source
            .get(end)
            .is_none_or(|byte| !is_ascii_identifier_continue(*byte))
}

fn is_ascii_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::IdentifierMatcher;

    #[test]
    fn identifier_boundaries() {
        let matcher = IdentifierMatcher::new("x");
        assert!(matcher.may_match("x"));
        assert!(matcher.may_match("x = 1"));
        assert!(matcher.may_match("obj.x"));
        assert!(matcher.may_match("x()"));
        assert!(matcher.may_match("exclude = x"));
        assert!(matcher.may_match("xx + x"));

        assert!(!matcher.may_match(""));
        assert!(!matcher.may_match("exclude = 10"));
        assert!(!matcher.may_match("ax"));
        assert!(!matcher.may_match("x1"));
        assert!(!matcher.may_match("1x"));
        assert!(!matcher.may_match("_x"));
        assert!(!matcher.may_match("x_"));
    }

    #[test]
    fn strings_and_comments_are_candidates() {
        let matcher = IdentifierMatcher::new("name");
        assert!(matcher.may_match(r#""name""#));
        assert!(matcher.may_match("# name"));
    }

    #[test]
    fn non_ascii_source_is_a_candidate() {
        let matcher = IdentifierMatcher::new("C");
        assert!(matcher.may_match("𝒞 = 1"));
        assert!(matcher.may_match("# café"));
    }
}
