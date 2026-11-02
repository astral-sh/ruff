use memchr::memmem::Finder;
use unicode_normalization::is_nfkc;

/// A reusable text prefilter for an identifier or keyword.
///
/// A match only indicates that a source may contain the name: matches in
/// comments and strings are included. Matchers created with [`Self::new`] accept
/// sources that change under NFKC normalization because Python normalizes
/// identifiers this way. Matchers created with [`Self::keyword`] search their literal
/// spelling instead. Callers should validate candidates using the AST or semantic analysis.
///
/// Construct a matcher once and reuse it across sources to avoid preprocessing
/// the name for each source. Cache matchers for fixed names in a [`std::sync::LazyLock`].
#[derive(Debug, Clone)]
pub struct NameMatcher<'a> {
    finder: Finder<'a>,
    is_keyword: bool,
}

impl<'a> NameMatcher<'a> {
    /// Creates an identifier matcher that borrows `name` without allocating.
    ///
    /// The identifier `name` is expected to be NFKC-normalized.
    pub fn new(name: &'a str) -> Self {
        Self {
            finder: Finder::new(name),
            is_keyword: false,
        }
    }

    /// Creates a keyword matcher that borrows `keyword` without allocating.
    ///
    /// Python recognizes keywords by their literal spelling, without NFKC normalization,
    /// so keyword matchers skip the normalization check.
    pub fn keyword(keyword: &'a str) -> Self {
        Self {
            finder: Finder::new(keyword),
            is_keyword: true,
        }
    }

    /// Returns whether `source` may contain the configured identifier or keyword.
    ///
    /// Identifier matchers conservatively return `true` if the source changes under
    /// NFKC normalization. Otherwise, searches for the literal spelling bounded by bytes
    /// other than ASCII letters, digits or `_`. Matches may occur in comments, strings,
    /// or Unicode identifiers.
    pub fn may_match(&self, source: &str) -> bool {
        if !self.is_keyword && !source.is_ascii() && !is_nfkc(source) {
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
    use super::NameMatcher;

    #[test]
    fn identifier_boundaries() {
        let matcher = NameMatcher::new("x");
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
        let matcher = NameMatcher::new("name");
        assert!(matcher.may_match(r#""name""#));
        assert!(matcher.may_match("# name"));
    }

    #[test]
    fn normalized_unicode_uses_literal_spelling() {
        let matcher = NameMatcher::new("C");
        assert!(!matcher.may_match("# note — a comment"));
        assert!(!matcher.may_match("# café"));
        assert!(!matcher.may_match("# 中文"));
        assert!(!matcher.may_match("# שלום"));
        assert!(!matcher.may_match("# 🦀"));
        // There is no precomposed character for 'q' with an acute accent.
        assert!(!matcher.may_match("# q\u{301}"));
        assert!(matcher.may_match("# café\nC = 1"));
        assert!(NameMatcher::new("café").may_match("café = 1"));
    }

    #[test]
    fn normalization_can_change_identifiers() {
        let matcher = NameMatcher::new("C");
        assert!(matcher.may_match("𝒞 = 1"));
        assert!(matcher.may_match("Ｃ = 1"));
        assert!(NameMatcher::new("ffi").may_match("ﬃ = 1"));
        assert!(NameMatcher::new("café").may_match("cafe\u{301} = 1"));
    }

    #[test]
    fn keyword_uses_literal_spelling() {
        let matcher = NameMatcher::keyword("class");
        assert!(matcher.may_match("class C: pass"));
        assert!(matcher.may_match("class Café: pass"));
        assert!(matcher.may_match("# class"));
        assert!(matcher.may_match(r#""class""#));
        assert!(matcher.may_match("éclass = 1"));
        assert!(!matcher.may_match("# café"));
        assert!(!matcher.may_match("ｃｌａｓｓ = 1"));
        assert!(!matcher.may_match("subclass = 1"));
    }
}
