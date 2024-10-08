//! Parse type and type-error assertions in Python comment form.
//!
//! Parses comments of the form `# Type: SomeType` and `# Error: 8 [rule-code] "message text"`. In
//! the latter case, the `8` is a column number, and `"message text"` asserts that the full
//! diagnostic message contains the text `"message text"`; all three are optional (`# Error:` will
//! match any error.)
//!
//! Assertion comments may be placed at end-of-line:
//!
//! ```py
//! x: int = "foo"  # Error: [invalid-assignment]
//! ```
//!
//! Or as a full-line comment on the preceding line:
//!
//! ```py
//! # Error: [invalid-assignment]
//! x: int = "foo"
//! ```
//!
//! Multiple assertion comments may apply to the same line; in this case all (or all but the last)
//! must be full-line comments:
//!
//! ```py
//! # Error: [unbound-name]
//! reveal_type(x)  # Type: Unbound
//! ```
//!
//! or
//!
//! ```py
//! # Error: [unbound-name]
//! # Type: Unbound
//! reveal_type(x)
//! ```
use crate::db::Db;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text, SourceText};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::{LineIndex, Locator, OneIndexed};
use ruff_text_size::{Ranged, TextRange};
use smallvec::SmallVec;
use std::ops::Deref;

/// Diagnostic assertion comments in a file.
#[derive(Debug)]
pub(crate) struct FileAssertions {
    comment_ranges: CommentRanges,
    source: SourceText,
    lines: LineIndex,
}

impl FileAssertions {
    pub(crate) fn from_file(db: &Db, file: File) -> Self {
        let source = source_text(db, file);
        let lines = line_index(db, file);
        let parsed = parsed_module(db, file);
        let comment_ranges = CommentRanges::from(parsed.tokens());
        Self {
            comment_ranges,
            source,
            lines,
        }
    }

    fn line_number(&self, range: &impl Ranged) -> OneIndexed {
        self.lines.line_index(range.start())
    }

    fn is_own_line(&self, range: &impl Ranged) -> bool {
        CommentRanges::is_own_line(range.start(), &Locator::new(&self.source))
    }
}

impl<'a> IntoIterator for &'a FileAssertions {
    type Item = LineAssertions<'a>;
    type IntoIter = LineAssertionsIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        Self::IntoIter {
            file_assertions: self,
            inner: AssertionWithRangeIterator {
                file_assertions: self,
                inner: self.comment_ranges.into_iter(),
            }
            .peekable(),
        }
    }
}

#[derive(Debug)]
struct AssertionWithRange<'a>(Assertion<'a>, TextRange);

#[derive(Debug)]
struct AssertionWithRangeIterator<'a> {
    file_assertions: &'a FileAssertions,
    inner: std::iter::Copied<std::slice::Iter<'a, TextRange>>,
}

impl<'a> Iterator for AssertionWithRangeIterator<'a> {
    type Item = AssertionWithRange<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let locator = Locator::new(&self.file_assertions.source);
        loop {
            let inner_next = self.inner.next()?;
            let comment = locator.slice(inner_next);
            if let Some(assertion) = Assertion::from_comment(comment) {
                return Some(AssertionWithRange(assertion, inner_next));
            };
        }
    }
}

impl std::iter::FusedIterator for AssertionWithRangeIterator<'_> {}

impl<'a> Deref for AssertionWithRange<'a> {
    type Target = Assertion<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Ranged for AssertionWithRange<'_> {
    fn range(&self) -> TextRange {
        self.1
    }
}

impl<'a> From<AssertionWithRange<'a>> for Assertion<'a> {
    fn from(value: AssertionWithRange<'a>) -> Self {
        value.0
    }
}

/// A vector of [`Assertion`]s belonging to a single line.
///
/// Most lines will have zero or one assertion, so we use a [`SmallVec`] optimized for a single
/// element to avoid most heap vector allocations.
type AssertionVec<'a> = SmallVec<[Assertion<'a>; 1]>;

#[derive(Debug)]
pub(crate) struct LineAssertionsIterator<'a> {
    file_assertions: &'a FileAssertions,
    inner: std::iter::Peekable<AssertionWithRangeIterator<'a>>,
}

impl<'a> Iterator for LineAssertionsIterator<'a> {
    type Item = LineAssertions<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file_assertions;
        let ranged_assertion = self.inner.next()?;
        let mut collector = AssertionVec::new();
        let mut line = file.line_number(&ranged_assertion);
        // Collect all own-line comments on consecutive lines; these all apply to the same line of
        // code. For example:
        //
        //     # Error: [unbound-name]
        //     # Type: Unbound
        //     reveal_type(x)
        //
        if file.is_own_line(&ranged_assertion) {
            collector.push(ranged_assertion.into());
            let mut only_own_line = true;
            while let Some(ranged_assertion) = self.inner.peek() {
                let next_line = line.saturating_add(1);
                if file.line_number(ranged_assertion) == next_line {
                    if !file.is_own_line(ranged_assertion) {
                        only_own_line = false;
                    }
                    line = next_line;
                    collector.push(self.inner.next().unwrap().into());
                    // If we see an end-of-line comment, it has to be the end of the stack,
                    // otherwise we'd botch this case, attributing all three errors to the `bar`
                    // line:
                    //
                    //     # Error:
                    //     foo  # Error:
                    //     bar  # Error:
                    if !only_own_line {
                        break;
                    }
                } else {
                    break;
                }
            }
            if only_own_line {
                // The collected comments apply to the _next_ line in the code.
                line = line.saturating_add(1);
            }
        } else {
            // We have a line-trailing comment; it applies to its own line, and is not grouped.
            collector.push(ranged_assertion.into());
        }
        Some(LineAssertions {
            line,
            assertions: collector,
        })
    }
}

impl std::iter::FusedIterator for LineAssertionsIterator<'_> {}

/// One or more assertions referring to the same line of code.
#[derive(Debug)]
pub(crate) struct LineAssertions<'a> {
    /// The line these assertions refer to.
    ///
    /// Not necessarily the same line the assertion comment is located on; for an own-line comment,
    /// it's the next non-assertion line.
    pub(crate) line: OneIndexed,

    /// The assertions referring to this line.
    pub(crate) assertions: AssertionVec<'a>,
}

impl<'a, 'b> From<&'a LineAssertions<'b>> for &'a [Assertion<'b>] {
    fn from(value: &'a LineAssertions<'b>) -> Self {
        &value.assertions[..]
    }
}

static TYPE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^#\s*Type:\s*(?<ty_display>.+?)\s*$").unwrap());

static ERROR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^#\s*Error:(\s*(?<column>\d+))?(\s*\[(?<rule>.+?)\])?(\s*"(?<message>.+?)")?\s*$"#,
    )
    .unwrap()
});

/// A single diagnostic assertion comment.
#[derive(Debug)]
pub(crate) enum Assertion<'a> {
    /// A `Type: ` assertion.
    Type(&'a str),

    /// An `Error: ` assertion.
    Error(ErrorAssertion<'a>),
}

impl<'a> Assertion<'a> {
    fn from_comment(comment: &'a str) -> Option<Self> {
        if let Some(caps) = TYPE_RE.captures(comment) {
            Some(Self::Type(caps.name("ty_display").unwrap().as_str()))
        } else {
            ERROR_RE.captures(comment).map(|caps| {
                Self::Error(ErrorAssertion {
                    rule: caps.name("rule").map(|m| m.as_str()),
                    column: caps.name("column").and_then(|m| m.as_str().parse().ok()),
                    message_contains: caps.name("message").map(|m| m.as_str()),
                })
            })
        }
    }
}

impl std::fmt::Display for Assertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Type(expected_type) => write!(f, "Type: {expected_type}"),
            Self::Error(assertion) => assertion.fmt(f),
        }
    }
}

/// An `Error: ` assertion comment.
#[derive(Debug)]
pub(crate) struct ErrorAssertion<'a> {
    /// The diagnostic rule code we expect.
    pub(crate) rule: Option<&'a str>,

    /// The column we expect the diagnostic range to start at.
    pub(crate) column: Option<OneIndexed>,

    /// A string we expect to be contained in the diagnostic message.
    pub(crate) message_contains: Option<&'a str>,
}

impl std::fmt::Display for ErrorAssertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Error:")?;
        if let Some(column) = self.column {
            write!(f, " {column}")?;
        }
        if let Some(rule) = self.rule {
            write!(f, " [{rule}]")?;
        }
        if let Some(message) = self.message_contains {
            write!(f, r#" "{message}""#)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Assertion, FileAssertions, LineAssertions};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_python_trivia::textwrap::dedent;
    use ruff_source_file::OneIndexed;

    fn get_assertions(source: &str) -> FileAssertions {
        let mut db = crate::db::Db::setup(SystemPathBuf::from("/src"));
        db.write_file("/src/test.py", source).unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        FileAssertions::from_file(&db, file)
    }

    fn as_vec(assertions: &FileAssertions) -> Vec<LineAssertions> {
        assertions.into_iter().collect()
    }

    #[test]
    fn ty_display() {
        let assertions = get_assertions(&dedent(
            "
            reveal_type(1)  # Type: Literal[1]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "Type: Literal[1]");
    }

    #[test]
    fn error() {
        let assertions = get_assertions(&dedent(
            "
            x  # Error:
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "Error:");
    }

    #[test]
    fn prior_line() {
        let assertions = get_assertions(&dedent(
            "
            # Type: Literal[1]
            reveal_type(1)
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "Type: Literal[1]");
    }

    #[test]
    fn stacked_prior_line() {
        let assertions = get_assertions(&dedent(
            "
            # Type: Unbound
            # Error: [unbound-name]
            reveal_type(x)
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(3));

        let [assert1, assert2] = &line.assertions[..] else {
            panic!("expected two assertions");
        };

        assert_eq!(format!("{assert1}"), "Type: Unbound");
        assert_eq!(format!("{assert2}"), "Error: [unbound-name]");
    }

    #[test]
    fn stacked_mixed() {
        let assertions = get_assertions(&dedent(
            "
            # Type: Unbound
            reveal_type(x) # Error: [unbound-name]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(2));

        let [assert1, assert2] = &line.assertions[..] else {
            panic!("expected two assertions");
        };

        assert_eq!(format!("{assert1}"), "Type: Unbound");
        assert_eq!(format!("{assert2}"), "Error: [unbound-name]");
    }

    #[test]
    fn multiple_lines() {
        let assertions = get_assertions(&dedent(
            r#"
            # Error: [invalid-assignment]
            x: int = "foo"
            y  # Error: [unbound-name]
            "#,
        ));

        let [line1, line2] = &as_vec(&assertions)[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line, OneIndexed::from_zero_indexed(2));
        assert_eq!(line2.line, OneIndexed::from_zero_indexed(3));

        let [Assertion::Error(error1)] = &line1.assertions[..] else {
            panic!("expected one Error assertion");
        };

        assert_eq!(error1.rule, Some("invalid-assignment"));

        let [Assertion::Error(error2)] = &line2.assertions[..] else {
            panic!("expected one Error assertion");
        };

        assert_eq!(error2.rule, Some("unbound-name"));
    }

    #[test]
    fn multiple_lines_mixed_stack() {
        let assertions = get_assertions(&dedent(
            r#"
            # Error: [invalid-assignment]
            x: int = reveal_type("foo")  # Type: str
            y  # Error: [unbound-name]
            "#,
        ));

        let [line1, line2] = &as_vec(&assertions)[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line, OneIndexed::from_zero_indexed(2));
        assert_eq!(line2.line, OneIndexed::from_zero_indexed(3));

        let [Assertion::Error(error1), Assertion::Type(expected_ty)] = &line1.assertions[..] else {
            panic!("expected one Error assertion and one Type assertion");
        };

        assert_eq!(error1.rule, Some("invalid-assignment"));
        assert_eq!(*expected_ty, "str");

        let [Assertion::Error(error2)] = &line2.assertions[..] else {
            panic!("expected one Error assertion");
        };

        assert_eq!(error2.rule, Some("unbound-name"));
    }

    #[test]
    fn error_with_rule() {
        let assertions = get_assertions(&dedent(
            "
            x  # Error: [unbound-name]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "Error: [unbound-name]");
    }

    #[test]
    fn error_with_rule_and_column() {
        let assertions = get_assertions(&dedent(
            "
            x  # Error: 1 [unbound-name]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "Error: 1 [unbound-name]");
    }

    #[test]
    fn error_with_rule_and_message() {
        let assertions = get_assertions(&dedent(
            r#"
            # Error: [unbound-name] "`x` is unbound"
            x
            "#,
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(
            format!("{assert}"),
            r#"Error: [unbound-name] "`x` is unbound""#
        );
    }

    #[test]
    fn error_with_message_and_column() {
        let assertions = get_assertions(&dedent(
            r#"
            # Error: 1 "`x` is unbound"
            x
            "#,
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), r#"Error: 1 "`x` is unbound""#);
    }

    #[test]
    fn error_with_rule_and_message_and_column() {
        let assertions = get_assertions(&dedent(
            r#"
            # Error: 1 [unbound-name] "`x` is unbound"
            x
            "#,
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(
            format!("{assert}"),
            r#"Error: 1 [unbound-name] "`x` is unbound""#
        );
    }
}
