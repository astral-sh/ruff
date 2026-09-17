use aho_corasick::{AhoCorasick, BuildError};
use memchr::memmem::Finder;

/// A reusable text prefilter for one or more identifier names.
///
/// A match only indicates that a source may contain one of the names: matches in
/// comments and strings are included, and non-ASCII sources always match because
/// Python normalizes identifiers with NFKC. Callers should validate candidates
/// using the AST or semantic analysis.
///
/// Construct a matcher once and reuse it across sources. Construction preprocesses
/// the names and, especially for multiple names, can be expensive. Cache matchers
/// for fixed names in a [`std::sync::LazyLock`].
#[derive(Debug, Clone)]
pub struct IdentifierMatcher<'a> {
    searcher: Searcher<'a>,
}

impl<'a> IdentifierMatcher<'a> {
    /// Creates a matcher for any of `names`.
    ///
    /// A single name is borrowed without allocating. Multiple names are compiled
    /// into an owned searcher.
    ///
    /// # Errors
    ///
    /// Returns an error if the names exceed Aho-Corasick's pattern count or size limits.
    pub fn new(names: impl AsRef<[&'a str]>) -> Result<Self, BuildError> {
        let searcher = match names.as_ref() {
            [name] => Searcher::Single(Finder::new(*name)),
            names => Searcher::Multiple(AhoCorasick::new(names)?),
        };
        Ok(Self { searcher })
    }

    /// Creates a matcher for one name without allocating.
    ///
    /// Reuse this matcher when searching multiple sources to avoid preprocessing
    /// the name for each source.
    pub fn single(name: &'a str) -> Self {
        Self {
            searcher: Searcher::Single(Finder::new(name)),
        }
    }

    /// Returns whether `source` may contain any of the configured identifiers.
    ///
    /// Returns `true` immediately for non-ASCII source. Otherwise, searches for
    /// literal spellings bounded by bytes other than ASCII letters, digits or `_`.
    pub fn may_match(&self, source: &str) -> bool {
        if !source.is_ascii() {
            return true;
        }

        let bytes = source.as_bytes();
        match &self.searcher {
            Searcher::Single(finder) => {
                let len = finder.needle().len();
                finder
                    .find_iter(bytes)
                    .any(|start| has_identifier_boundaries(bytes, start, start + len))
            }
            Searcher::Multiple(searcher) => {
                // With names like `foo` and `foobar`, rejecting the shorter match
                // for its trailing boundary must not hide the longer match.
                searcher
                    .find_overlapping_iter(bytes)
                    .any(|matched| has_identifier_boundaries(bytes, matched.start(), matched.end()))
            }
        }
    }
}

#[derive(Debug, Clone)]
#[allow(
    clippy::large_enum_variant,
    reason = "Finder's size depends on the target; keep single-name searches allocation-free."
)]
enum Searcher<'a> {
    Single(Finder<'a>),
    Multiple(AhoCorasick),
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
    fn single_identifier_boundaries() {
        let matcher = IdentifierMatcher::single("x");
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
    fn multiple_identifier_boundaries() -> Result<(), aho_corasick::BuildError> {
        let matcher = IdentifierMatcher::new(["x", "other"])?;
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
        Ok(())
    }

    #[test]
    fn matches_any_name() -> Result<(), aho_corasick::BuildError> {
        let matcher =
            IdentifierMatcher::new(["pkgutil", "pkg_resources", "setuptools", "importlib"])?;
        assert!(matcher.may_match("import pkgutil"));
        assert!(matcher.may_match(r#"__import__("pkg_resources")"#));
        assert!(matcher.may_match("import setuptools"));
        assert!(matcher.may_match("import importlib"));
        assert!(!matcher.may_match("import logging"));
        assert!(!matcher.may_match("my_pkgutil = pkg_resources_helper"));
        Ok(())
    }

    #[test]
    fn strings_and_comments_are_candidates() {
        let matcher = IdentifierMatcher::single("name");
        assert!(matcher.may_match(r#""name""#));
        assert!(matcher.may_match("# name"));
    }

    #[test]
    fn overlapping_names_shorter_first() -> Result<(), aho_corasick::BuildError> {
        let matcher = IdentifierMatcher::new(["foo", "foobar"])?;
        assert!(matcher.may_match("foobar"));
        assert!(!matcher.may_match("foobars"));
        Ok(())
    }

    #[test]
    fn overlapping_names_longer_first() -> Result<(), aho_corasick::BuildError> {
        let matcher = IdentifierMatcher::new(["foobar", "foo"])?;
        assert!(matcher.may_match("foobar"));
        assert!(!matcher.may_match("foobars"));
        Ok(())
    }

    #[test]
    fn overlapping_names_suffix() -> Result<(), aho_corasick::BuildError> {
        let matcher = IdentifierMatcher::new(["bar", "foobar"])?;
        assert!(matcher.may_match("foobar"));
        assert!(!matcher.may_match("foobars"));
        Ok(())
    }

    #[test]
    fn non_ascii_source_single_name() {
        let matcher = IdentifierMatcher::single("C");
        assert!(matcher.may_match("𝒞 = 1"));
        assert!(matcher.may_match("# café"));
    }

    #[test]
    fn non_ascii_source_multiple_names() -> Result<(), aho_corasick::BuildError> {
        let matcher = IdentifierMatcher::new(["C", "D"])?;
        assert!(matcher.may_match("𝒞 = 1"));
        assert!(matcher.may_match("# café"));
        Ok(())
    }
}
