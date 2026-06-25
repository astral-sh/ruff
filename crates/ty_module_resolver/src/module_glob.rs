//! Module glob patterns for matching Python module names.
//!
//! This module provides glob-like pattern matching for Python module names,
//! allowing configuration of which modules should be included or excluded
//! from certain behaviors (e.g., ignoring import resolution errors).
//!
//! # Examples
//!
//! ```
//! use ty_module_resolver::{ModuleGlobSet, ModuleName};
//!
//! let set = ModuleGlobSet::from_patterns(["test.*", "!test.internal"]).unwrap();
//!
//! assert!(set.matches(&ModuleName::new("test.foo").unwrap()).is_include());
//! assert!(set.matches(&ModuleName::new("test.internal").unwrap()).is_exclude());
//! ```
//!
//! # Pattern Syntax
//!
//! - `test` matches the module `test` exactly (but not `test.foo`).
//!
//! - `*` matches zero or more characters, but not `.`.
//!
//! - `**` matches zero or more module components. This sequence **must** form
//!   a single component, so both `**foo` and `foo**` are invalid and will
//!   result in an error. A sequence of more than two consecutive `*` characters
//!   is also invalid.
//!
//! - Patterns starting with `!` are negated and will exclude matching modules.
//!   When multiple patterns match, the last match wins (like gitignore).
//!
//! # Pattern Examples
//!
//! | Pattern | Matches | Does not match |
//! |---------|---------|----------------|
//! | `test` | `test` | `test.foo`, `testing` |
//! | `test.*` | `test.foo`, `test.bar` | `test`, `test.foo.bar` |
//! | `*.test` | `foo.test`, `bar.test` | `test`, `foo.bar.test` |
//! | `test.**` | `test`, `test.foo`, `test.foo.bar` | `testing` |
//! | `**.test` | `test`, `foo.test`, `foo.bar.test` | `test.foo` |
//! | `test.**.bar` | `test.bar`, `test.foo.bar`, `test.a.b.bar` | `test`, `test.bar.foo` |
//! | `**` | (any module) | |

use std::fmt;

use regex::RegexSet;

use crate::ModuleName;

/// A compiled set of module glob patterns.
///
/// This allows efficient matching of module names against multiple glob patterns,
/// with support for negated patterns.
#[derive(Clone, Debug, get_size2::GetSize)]
pub struct ModuleGlobSet {
    #[get_size(ignore)]
    regex_set: RegexSet,
    /// Parsed glob metadata.
    globs: Box<[ModuleGlob]>,
}

impl ModuleGlobSet {
    pub fn empty() -> Self {
        Self {
            regex_set: RegexSet::empty(),
            globs: Box::default(),
        }
    }

    /// Creates a new [`ModuleGlobSet`] from an iterator of patterns.
    ///
    /// This is a convenience method that creates a builder, adds all patterns,
    /// and builds the set.
    ///
    /// # Errors
    ///
    /// Returns an error if any pattern is invalid or if the regex set fails to compile.
    pub fn from_patterns<I, S>(patterns: I) -> Result<Self, ModuleGlobError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut builder = ModuleGlobSetBuilder::new();
        for pattern in patterns {
            builder.add(pattern.as_ref())?;
        }
        builder.build()
    }

    /// Returns whether the given module name matches any pattern in this set.
    ///
    /// Uses "last match wins" semantics (like gitignore):
    /// - Returns [`ModuleNameMatch::Include`] if the last matching pattern is a positive pattern.
    /// - Returns [`ModuleNameMatch::Exclude`] if the last matching pattern is a negative pattern.
    /// - Returns [`ModuleNameMatch::None`] if no pattern matches.
    pub fn matches(&self, module: &ModuleName) -> ModuleNameMatch {
        if self.globs.is_empty() {
            return ModuleNameMatch::None;
        }

        // Find the last matching pattern (by index order, which is the order patterns were added).
        let Some(last_match_index) = self.regex_set.matches(module.as_str()).iter().next_back()
        else {
            return ModuleNameMatch::None;
        };

        if self.globs[last_match_index].negated {
            ModuleNameMatch::Exclude
        } else {
            ModuleNameMatch::Include
        }
    }
}

impl PartialEq for ModuleGlobSet {
    fn eq(&self, other: &Self) -> bool {
        self.globs == other.globs
    }
}

impl Eq for ModuleGlobSet {}

impl fmt::Display for ModuleGlobSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.globs.iter().map(|g| &g.original))
            .finish()
    }
}

/// Builder for constructing a [`ModuleGlobSet`].
///
/// For simple cases, prefer [`ModuleGlobSet::from_patterns`] instead.
#[derive(Debug, Default)]
pub struct ModuleGlobSetBuilder {
    /// Regex patterns converted from globs.
    patterns: Vec<Box<str>>,
    /// Parsed glob metadata.
    globs: Vec<ModuleGlob>,
}

impl ModuleGlobSetBuilder {
    /// Creates a new empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a glob pattern to the builder.
    ///
    /// Patterns starting with `!` are treated as negated patterns.
    ///
    /// # Errors
    ///
    /// Returns an error if the pattern is invalid.
    pub fn add(&mut self, pattern: &str) -> Result<&mut Self, ModuleGlobError> {
        if pattern.is_empty() {
            return Err(ModuleGlobError::EmptyPattern);
        }

        // Handle negation prefix.
        let (negated, pattern_without_negation) = if let Some(rest) = pattern.strip_prefix('!') {
            (true, rest)
        } else {
            (false, pattern)
        };

        if pattern_without_negation.is_empty() {
            return Err(ModuleGlobError::EmptyPattern);
        }

        let regex_pattern = glob_to_regex(pattern_without_negation)?;

        self.patterns.push(regex_pattern);
        self.globs.push(ModuleGlob {
            original: pattern.into(),
            negated,
        });

        Ok(self)
    }

    /// Builds the [`ModuleGlobSet`] from the added patterns.
    ///
    /// # Errors
    ///
    /// Returns an error if the regex set fails to compile.
    pub fn build(self) -> Result<ModuleGlobSet, ModuleGlobError> {
        let regex_set = RegexSet::new(&self.patterns)?;

        Ok(ModuleGlobSet {
            regex_set,
            globs: self.globs.into_boxed_slice(),
        })
    }
}

/// The result of matching a module name against a [`ModuleGlobSet`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ModuleNameMatch {
    /// The module name matches no pattern.
    None,

    /// The module name matches an include pattern (a positive pattern).
    Include,

    /// The module name matches an exclude pattern (a negative pattern starting with `!`).
    Exclude,
}

impl ModuleNameMatch {
    /// Returns `true` if the match result is [`ModuleNameMatch::Include`].
    pub const fn is_include(self) -> bool {
        matches!(self, ModuleNameMatch::Include)
    }

    /// Returns `true` if the match result is [`ModuleNameMatch::Exclude`].
    pub const fn is_exclude(self) -> bool {
        matches!(self, ModuleNameMatch::Exclude)
    }

    /// Returns `true` if the match result is [`ModuleNameMatch::None`].
    pub const fn is_none(self) -> bool {
        matches!(self, ModuleNameMatch::None)
    }
}

/// Error type for module glob pattern parsing.
#[derive(Debug, thiserror::Error)]
pub enum ModuleGlobError {
    /// The pattern is empty (e.g., `""` or `"!"`).
    #[error("module glob pattern cannot be empty")]
    EmptyPattern,

    /// The pattern starts with a dot (e.g., `.foo`).
    #[error("module glob pattern cannot start with a dot")]
    LeadingDot,

    /// The pattern ends with a dot (e.g., `foo.`).
    #[error("module glob pattern cannot end with a dot")]
    TrailingDot,

    /// The pattern contains consecutive dots (e.g., `foo..bar`).
    #[error("module glob pattern cannot contain consecutive dots")]
    ConsecutiveDots,

    /// The pattern contains an invalid `**` usage (e.g., `foo**` or `**bar`).
    #[error(
        "`**` can only appear as a complete component (e.g., `foo.**` or `**.bar`), not combined with other text like `{0}`"
    )]
    InvalidDoubleStarUsage(Box<str>),

    /// The underlying regex failed to compile (e.g., DFA size limit exceeded).
    #[error("failed to compile module glob pattern")]
    Regex(#[from] regex::Error),
}

/// A parsed module glob pattern.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
struct ModuleGlob {
    /// The original glob pattern string (including `!` prefix if negated).
    original: Box<str>,
    /// Whether this is a negated pattern (starts with `!`).
    negated: bool,
}

/// Converts a module glob pattern to a regex pattern, validating during parsing.
fn glob_to_regex(pattern: &str) -> Result<Box<str>, ModuleGlobError> {
    if pattern.is_empty() {
        return Err(ModuleGlobError::EmptyPattern);
    }

    // Check for leading or trailing dots.
    if pattern.starts_with('.') {
        return Err(ModuleGlobError::LeadingDot);
    }
    if pattern.ends_with('.') {
        return Err(ModuleGlobError::TrailingDot);
    }

    let mut regex = String::with_capacity(pattern.len());
    regex.push('^');

    let mut components = pattern.split('.').peekable();

    let mut is_first = true;
    let mut prev_was_double_star_at_start = false;

    while let Some(component) = components.next() {
        if component.is_empty() {
            return Err(ModuleGlobError::ConsecutiveDots);
        }

        // Check for `**` mixed with other characters.
        if component.contains("**") && component != "**" {
            return Err(ModuleGlobError::InvalidDoubleStarUsage(Box::from(
                component,
            )));
        }

        let is_last = components.peek().is_none();

        if component == "**" {
            if is_first {
                // Pattern is just "**" - matches everything.
                if is_last {
                    regex.push_str(".*");
                } else {
                    // "**.foo" - matches zero or more components at the start.
                    // Matches: "foo", "x.foo", "x.y.foo", etc.
                    // The pattern includes the trailing dot if there are prefix components.
                    regex.push_str("(?:[^.]+\\.)*");
                    prev_was_double_star_at_start = true;
                }
            } else {
                // "foo.**" or "foo.**.bar" - matches zero or more components.
                // Matches: "foo", "foo.x", "foo.x.y", "foo.bar", "foo.x.bar", etc.
                regex.push_str("(?:\\.[^.]+)*");
            }
        } else {
            // Add dot separator if not at the beginning and not after `**` at the start.
            // When `**` is at position 0, it already includes the trailing dot in its pattern.
            if !is_first && !prev_was_double_star_at_start {
                regex.push_str("\\.");
            }
            prev_was_double_star_at_start = false;

            // Handle `*` as a complete component vs `*` mixed with text differently.
            if component == "*" {
                // `*` as a complete component matches exactly one non-empty component.
                regex.push_str("[^.]+");
            } else {
                // Convert the component, handling `*` wildcards mixed with text.
                for c in component.chars() {
                    if c == '*' {
                        // `*` mixed with text matches zero or more characters except `.`.
                        regex.push_str("[^.]*");
                    } else if regex_syntax::is_meta_character(c) {
                        // Escape regex special characters.
                        regex.push('\\');
                        regex.push(c);
                    } else {
                        regex.push(c);
                    }
                }
            }
        }

        is_first = false;
    }

    regex.push('$');
    Ok(regex.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn assert_include(set: &ModuleGlobSet, name: &str) {
        let module = ModuleName::new(name).unwrap();
        assert_eq!(
            set.matches(&module),
            ModuleNameMatch::Include,
            "expected `{name}` to be included"
        );
    }

    #[track_caller]
    fn assert_excludes(set: &ModuleGlobSet, name: &str) {
        let module = ModuleName::new(name).unwrap();
        assert_eq!(
            set.matches(&module),
            ModuleNameMatch::Exclude,
            "expected `{name}` to be excluded"
        );
    }

    #[track_caller]
    fn assert_no_match(set: &ModuleGlobSet, name: &str) {
        let module = ModuleName::new(name).unwrap();
        assert_eq!(
            set.matches(&module),
            ModuleNameMatch::None,
            "expected `{name}` not to match"
        );
    }

    #[test]
    fn test_exact_match() {
        let set = ModuleGlobSet::from_patterns(["test"]).unwrap();

        assert_include(&set, "test");
        assert_no_match(&set, "test2");
        assert_no_match(&set, "test_foo");
        assert_no_match(&set, "foo");
        assert_no_match(&set, "test.foo");
    }

    #[test]
    fn test_single_star_direct_submodule() {
        let set = ModuleGlobSet::from_patterns(["test.*"]).unwrap();

        assert_include(&set, "test.foo");
        assert_include(&set, "test.bar");
        assert_no_match(&set, "test");
        assert_no_match(&set, "test.foo.bar");
    }

    #[test]
    fn test_single_star_prefix() {
        let set = ModuleGlobSet::from_patterns(["*.test"]).unwrap();

        assert_include(&set, "foo.test");
        assert_include(&set, "bar.test");
        assert_no_match(&set, "test");
        assert_no_match(&set, "foo.bar.test");
    }

    #[test]
    fn test_single_star_middle() {
        let set = ModuleGlobSet::from_patterns(["foo.*.bar"]).unwrap();

        assert_include(&set, "foo.x.bar");
        assert_include(&set, "foo.y.bar");
        assert_no_match(&set, "foo.bar");
        assert_no_match(&set, "foo.x.y.bar");
    }

    #[test]
    fn test_star_with_literal_text() {
        let set = ModuleGlobSet::from_patterns(["*test.bar"]).unwrap();

        assert_include(&set, "test.bar");
        assert_include(&set, "mytest.bar");
        assert_no_match(&set, "foobar.bar");
    }

    #[test]
    fn test_double_star_end() {
        let set = ModuleGlobSet::from_patterns(["test.**"]).unwrap();

        assert_include(&set, "test");
        assert_include(&set, "test.foo");
        assert_include(&set, "test.foo.bar");
        assert_include(&set, "test.foo.bar.baz");
        assert_no_match(&set, "testing");
    }

    #[test]
    fn test_double_star_start() {
        let set = ModuleGlobSet::from_patterns(["**.bar"]).unwrap();

        assert_include(&set, "bar");
        assert_include(&set, "foo.bar");
        assert_include(&set, "foo.baz.bar");
        assert_include(&set, "foo.baz.qux.bar");
        assert_no_match(&set, "bar.foo");
    }

    #[test]
    fn test_double_star_middle() {
        let set = ModuleGlobSet::from_patterns(["test.**.bar"]).unwrap();

        assert_include(&set, "test.bar");
        assert_include(&set, "test.foo.bar");
        assert_include(&set, "test.foo.baz.bar");
        assert_include(&set, "test.foo.baz.qux.bar");
        assert_no_match(&set, "test");
        assert_no_match(&set, "test.bar.foo");
    }

    #[test]
    fn test_just_double_star() {
        let set = ModuleGlobSet::from_patterns(["**"]).unwrap();

        assert_include(&set, "foo");
        assert_include(&set, "foo.bar");
        assert_include(&set, "foo.bar.baz");
    }

    #[test]
    fn test_negated_pattern() {
        let set = ModuleGlobSet::from_patterns(["test.*", "!test.internal"]).unwrap();

        assert_include(&set, "test.foo");
        assert_include(&set, "test.bar");
        // Last match wins - !test.internal matches last, so it excludes.
        assert_excludes(&set, "test.internal");
    }

    #[test]
    fn test_negated_pattern_override() {
        // The negation comes first, but test.* comes last and overrides it.
        let set = ModuleGlobSet::from_patterns(["!test.internal", "test.*"]).unwrap();

        assert_include(&set, "test.foo");
        assert_include(&set, "test.bar");
        // test.* matches last, so test.internal is included.
        assert_include(&set, "test.internal");
    }

    #[test]
    fn test_negated_only() {
        let set = ModuleGlobSet::from_patterns(["!test"]).unwrap();

        assert_excludes(&set, "test");
        assert_no_match(&set, "other");
    }

    #[test]
    fn test_empty_set() {
        let set = ModuleGlobSet::from_patterns::<[&str; 0], _>([]).unwrap();

        assert_no_match(&set, "test");
    }

    #[test]
    fn test_display() {
        let set = ModuleGlobSet::from_patterns(["test.*", "!test.internal"]).unwrap();

        let display = format!("{set}");
        assert!(display.contains("test.*"));
        assert!(display.contains("!test.internal"));
    }

    #[test]
    fn test_invalid_empty_pattern() {
        let result = ModuleGlobSet::from_patterns([""]);
        assert!(matches!(result, Err(ModuleGlobError::EmptyPattern)));
    }

    #[test]
    fn test_invalid_just_negation() {
        let result = ModuleGlobSet::from_patterns(["!"]);
        assert!(matches!(result, Err(ModuleGlobError::EmptyPattern)));
    }

    #[test]
    fn test_invalid_double_star_combined() {
        let result = ModuleGlobSet::from_patterns(["foo**"]);
        assert!(matches!(
            result,
            Err(ModuleGlobError::InvalidDoubleStarUsage(_))
        ));

        let result = ModuleGlobSet::from_patterns(["**foo"]);
        assert!(matches!(
            result,
            Err(ModuleGlobError::InvalidDoubleStarUsage(_))
        ));

        let result = ModuleGlobSet::from_patterns(["foo.bar**"]);
        assert!(matches!(
            result,
            Err(ModuleGlobError::InvalidDoubleStarUsage(_))
        ));
    }

    #[test]
    fn test_invalid_consecutive_dots() {
        let result = ModuleGlobSet::from_patterns(["foo..bar"]);
        assert!(matches!(result, Err(ModuleGlobError::ConsecutiveDots)));
    }

    #[test]
    fn test_invalid_leading_dot() {
        let result = ModuleGlobSet::from_patterns([".foo"]);
        assert!(matches!(result, Err(ModuleGlobError::LeadingDot)));
    }

    #[test]
    fn test_invalid_trailing_dot() {
        let result = ModuleGlobSet::from_patterns(["foo."]);
        assert!(matches!(result, Err(ModuleGlobError::TrailingDot)));
    }

    #[test]
    fn test_underscore_in_module_name() {
        let set = ModuleGlobSet::from_patterns(["foo_bar.*"]).unwrap();

        assert_include(&set, "foo_bar.baz");
    }

    #[test]
    fn test_numbers_in_module_name() {
        let set = ModuleGlobSet::from_patterns(["foo123.*"]).unwrap();

        assert_include(&set, "foo123.bar");
    }

    #[test]
    fn test_multiple_patterns() {
        let set = ModuleGlobSet::from_patterns(["alpha.*", "beta.*", "gamma"]).unwrap();

        assert_include(&set, "alpha.one");
        assert_include(&set, "beta.two");
        assert_include(&set, "gamma");
        assert_no_match(&set, "delta");
    }
}
