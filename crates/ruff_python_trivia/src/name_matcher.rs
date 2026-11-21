use bstr::ByteSlice;
use memchr::memmem::Finder;
use unicode_normalization::UnicodeNormalization;

use crate::tokenizer::{is_identifier_continuation, is_identifier_start};

/// A reusable text prefilter for an identifier or keyword.
///
/// A match only indicates that a source may contain the name: matches in
/// comments and strings are included. Matchers created with [`Self::new`] accept
/// sources containing an identifier that normalizes to the searched name under NFKC.
/// Matchers created with [`Self::keyword`] look for the exact keyword text.
/// Callers should validate candidates using the AST or semantic analysis.
///
/// Construct a matcher once and reuse it across sources to avoid preprocessing
/// the name for each source. Cache matchers for fixed names in a [`std::sync::LazyLock`].
#[derive(Debug, Clone)]
pub struct NameMatcher<'a> {
    finder: Finder<'a>,
    identifier: Option<&'a str>,
}

impl<'a> NameMatcher<'a> {
    /// Creates an identifier matcher that borrows `name` without allocating.
    ///
    /// The identifier `name` is expected to be NFKC-normalized.
    pub fn new(name: &'a str) -> Self {
        Self {
            finder: Finder::new(name),
            identifier: Some(name),
        }
    }

    /// Creates a keyword matcher that borrows `keyword` without allocating.
    ///
    /// Python recognizes keywords before NFKC normalization, so keyword matchers
    /// compare the source text directly.
    pub fn keyword(keyword: &'a str) -> Self {
        Self {
            finder: Finder::new(keyword),
            identifier: None,
        }
    }

    /// Returns whether `source` may contain the configured identifier or keyword.
    ///
    /// The literal search checks for boundaries at bytes other than ASCII letters, digits,
    /// or `_`. Identifier matchers also check whether a whole non-ASCII identifier
    /// normalizes to the searched name.
    pub fn may_match(&self, source: &str) -> bool {
        let bytes = source.as_bytes();
        let len = self.finder.needle().len();
        if self
            .finder
            .find_iter(bytes)
            .any(|start| has_identifier_boundaries(bytes, start, start + len))
        {
            return true;
        }

        let Some(name) = self.identifier else {
            return false;
        };
        unicode_identifiers(source).any(|word| word.nfkc().eq(name.chars()))
    }
}

fn unicode_identifiers(mut source: &str) -> impl Iterator<Item = &str> {
    std::iter::from_fn(move || {
        loop {
            // Only identifiers containing a non-ASCII character can change under NFKC.
            // The first non-ASCII byte in a UTF-8 string always starts a character.
            let index = source.as_bytes().find_non_ascii_byte()?;
            let (ascii, rest) = source.split_at(index);
            let mut chars = rest.chars();
            let first = chars.next()?;
            if !is_identifier_continuation(first) {
                // Punctuation such as `…` separates identifiers. Resume just after it.
                source = chars.as_str();
                continue;
            }

            // In `obj.myＣache()`, we landed on `Ｃ`. Scan left over `my` and right
            // over `ache` so we yield the whole identifier and resume at `(`.
            let start = ascii.trim_end_matches(is_identifier_continuation).len();
            let remaining = chars
                .as_str()
                .trim_start_matches(is_identifier_continuation);
            let word = &source[start..source.len() - remaining.len()];
            source = remaining;
            // Skip runs like `1Ｃ`: a Python identifier cannot start with a digit.
            if word.starts_with(is_identifier_start) {
                return Some(word);
            }
        }
    })
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
    fn unrelated_unicode_does_not_match() {
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
    fn non_identifier_punctuation() {
        let matcher = NameMatcher::new("C");
        assert!(!matcher.may_match("# …"));
        assert!(matcher.may_match("obj.C  # …"));
    }

    #[test]
    fn normalized_whole_identifier() {
        let source = "obj.myＣache";
        assert!(!NameMatcher::new("C").may_match(source));
        assert!(NameMatcher::new("myCache").may_match(source));
        assert!(!NameMatcher::new("C").may_match("obj.Ｃβ"));
        assert!(!NameMatcher::new("f").may_match("obj.ﬃ"));
        assert!(NameMatcher::new("C").may_match(r#"x: "models.Ｃ""#));
    }

    #[test]
    fn skips_to_the_next_unicode_identifier() {
        let source = "ascii…1Ｃ 𝒞β / myＣache — cafe\u{301}";
        let words: Vec<_> = super::unicode_identifiers(source).collect();
        assert_eq!(words, ["𝒞β", "myＣache", "cafe\u{301}"]);
    }

    #[test]
    fn keyword_matches_exact_text() {
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
