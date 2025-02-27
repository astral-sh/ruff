//! Parse type and type-error assertions in Python comment form.
//!
//! Parses comments of the form `# revealed: SomeType` and `# error: 8 [rule-code] "message text"`.
//! In the latter case, the `8` is a column number, and `"message text"` asserts that the full
//! diagnostic message contains the text `"message text"`; all three are optional (`# error:` will
//! match any error.)
//!
//! Assertion comments may be placed at end-of-line:
//!
//! ```py
//! x: int = "foo"  # error: [invalid-assignment]
//! ```
//!
//! Or as a full-line comment on the preceding line:
//!
//! ```py
//! # error: [invalid-assignment]
//! x: int = "foo"
//! ```
//!
//! Multiple assertion comments may apply to the same line; in this case all (or all but the last)
//! must be full-line comments:
//!
//! ```py
//! # error: [unbound-name]
//! reveal_type(x)  # revealed: Unbound
//! ```
//!
//! or
//!
//! ```py
//! # error: [unbound-name]
//! # revealed: Unbound
//! reveal_type(x)
//! ```

use crate::db::Db;
use regex::Regex;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{line_index, source_text, SourceText};
use ruff_python_trivia::{CommentRanges, Cursor};
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use smallvec::SmallVec;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::LazyLock;

/// Diagnostic assertion comments in a single embedded file.
#[derive(Debug)]
pub(crate) struct InlineFileAssertions {
    comment_ranges: CommentRanges,
    source: SourceText,
    lines: LineIndex,
}

impl InlineFileAssertions {
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

    fn is_own_line_comment(&self, ranged_assertion: &AssertionWithRange) -> bool {
        CommentRanges::is_own_line(ranged_assertion.start(), self.source.as_str())
    }
}

impl<'a> IntoIterator for &'a InlineFileAssertions {
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

/// An [`Assertion`] with the [`TextRange`] of its original inline comment.
#[derive(Debug)]
struct AssertionWithRange<'a>(UnparsedAssertion<'a>, TextRange);

impl<'a> Deref for AssertionWithRange<'a> {
    type Target = UnparsedAssertion<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Ranged for AssertionWithRange<'_> {
    fn range(&self) -> TextRange {
        self.1
    }
}

impl<'a> From<AssertionWithRange<'a>> for UnparsedAssertion<'a> {
    fn from(value: AssertionWithRange<'a>) -> Self {
        value.0
    }
}

/// Iterator that yields all assertions within a single embedded Python file.
#[derive(Debug)]
struct AssertionWithRangeIterator<'a> {
    file_assertions: &'a InlineFileAssertions,
    inner: std::iter::Copied<std::slice::Iter<'a, TextRange>>,
}

impl<'a> Iterator for AssertionWithRangeIterator<'a> {
    type Item = AssertionWithRange<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let inner_next = self.inner.next()?;
            let comment = &self.file_assertions.source[inner_next];
            if let Some(assertion) = UnparsedAssertion::from_comment(comment) {
                return Some(AssertionWithRange(assertion, inner_next));
            };
        }
    }
}

impl std::iter::FusedIterator for AssertionWithRangeIterator<'_> {}

/// A vector of [`Assertion`]s belonging to a single line.
///
/// Most lines will have zero or one assertion, so we use a [`SmallVec`] optimized for a single
/// element to avoid most heap vector allocations.
type AssertionVec<'a> = SmallVec<[UnparsedAssertion<'a>; 1]>;

#[derive(Debug)]
pub(crate) struct LineAssertionsIterator<'a> {
    file_assertions: &'a InlineFileAssertions,
    inner: std::iter::Peekable<AssertionWithRangeIterator<'a>>,
}

impl<'a> Iterator for LineAssertionsIterator<'a> {
    type Item = LineAssertions<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file_assertions;
        let ranged_assertion = self.inner.next()?;
        let mut collector = AssertionVec::new();
        let mut line_number = file.line_number(&ranged_assertion);
        // Collect all own-line comments on consecutive lines; these all apply to the same line of
        // code. For example:
        //
        // ```py
        // # error: [unbound-name]
        // # revealed: Unbound
        // reveal_type(x)
        // ```
        //
        if file.is_own_line_comment(&ranged_assertion) {
            collector.push(ranged_assertion.into());
            let mut only_own_line = true;
            while let Some(ranged_assertion) = self.inner.peek() {
                let next_line_number = line_number.saturating_add(1);
                if file.line_number(ranged_assertion) == next_line_number {
                    if !file.is_own_line_comment(ranged_assertion) {
                        only_own_line = false;
                    }
                    line_number = next_line_number;
                    collector.push(self.inner.next().unwrap().into());
                    // If we see an end-of-line comment, it has to be the end of the stack,
                    // otherwise we'd botch this case, attributing all three errors to the `bar`
                    // line:
                    //
                    // ```py
                    // # error:
                    // foo  # error:
                    // bar  # error:
                    // ```
                    //
                    if !only_own_line {
                        break;
                    }
                } else {
                    break;
                }
            }
            if only_own_line {
                // The collected comments apply to the _next_ line in the code.
                line_number = line_number.saturating_add(1);
            }
        } else {
            // We have a line-trailing comment; it applies to its own line, and is not grouped.
            collector.push(ranged_assertion.into());
        }
        Some(LineAssertions {
            line_number,
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
    pub(crate) line_number: OneIndexed,

    /// The assertions referring to this line.
    pub(crate) assertions: AssertionVec<'a>,
}

impl<'a> Deref for LineAssertions<'a> {
    type Target = [UnparsedAssertion<'a>];

    fn deref(&self) -> &Self::Target {
        &self.assertions
    }
}

static TYPE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s*revealed:\s*(?<ty_display>.+?)\s*$").unwrap());

const ERROR_PREFIX: &str = "# error: ";

/// A single diagnostic assertion comment.
#[derive(Debug)]
pub(crate) enum UnparsedAssertion<'a> {
    /// A `revealed: ` assertion.
    Revealed(&'a str),

    /// An `error: ` assertion.
    Error(&'a str),
}

impl<'a> UnparsedAssertion<'a> {
    fn from_comment(comment: &'a str) -> Option<Self> {
        if let Some(caps) = TYPE_RE.captures(comment) {
            Some(Self::Revealed(caps.name("ty_display").unwrap().as_str()))
        } else {
            let (prefix, assertion) = comment.split_at_checked(ERROR_PREFIX.len())?;
            (prefix == ERROR_PREFIX).then_some(Self::Error(assertion))
        }
    }

    pub(crate) fn parse(&self) -> Result<ParsedAssertion<'a>, String> {
        match self {
            Self::Revealed(revealed) => Ok(ParsedAssertion::Revealed(revealed)),
            Self::Error(error) => ErrorAssertion::from_str(error).map(ParsedAssertion::Error),
        }
    }
}

impl std::fmt::Display for UnparsedAssertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Revealed(expected_type) => write!(f, "revealed: {expected_type}"),
            Self::Error(assertion) => write!(f, "error: {assertion}"),
        }
    }
}

#[derive(Debug)]
pub(crate) enum ParsedAssertion<'a> {
    Revealed(&'a str),
    Error(ErrorAssertion<'a>),
}

impl std::fmt::Display for ParsedAssertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Revealed(expected_type) => write!(f, "revealed: {expected_type}"),
            Self::Error(assertion) => assertion.fmt(f),
        }
    }
}

/// An `error: ` assertion comment.
#[derive(Debug)]
pub(crate) struct ErrorAssertion<'a> {
    /// The diagnostic rule code we expect.
    pub(crate) rule: Option<&'a str>,

    /// The column we expect the diagnostic range to start at.
    pub(crate) column: Option<OneIndexed>,

    /// A string we expect to be contained in the diagnostic message.
    pub(crate) message_contains: Option<&'a str>,
}

impl<'a> ErrorAssertion<'a> {
    fn from_str(source: &'a str) -> Result<Self, String> {
        ErrorAssertionParser::new(source).parse()
    }
}

impl std::fmt::Display for ErrorAssertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("error:")?;
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

#[derive(Debug, Clone)]
pub(crate) struct ErrorAssertionParser<'a> {
    cursor: Cursor<'a>,
    source: &'a str,
}

impl<'a> ErrorAssertionParser<'a> {
    fn new(comment: &'a str) -> Self {
        Self {
            cursor: Cursor::new(comment),
            source: comment,
        }
    }

    /// Retrieves the current offset of the cursor within the source code.
    fn offset(&self) -> TextSize {
        self.source.text_len() - self.cursor.text_len()
    }

    fn skip_whitespace(&mut self) {
        self.cursor.eat_while(|c| c.is_whitespace() && c != '\n');
    }

    fn consume_until(&mut self, mut end_predicate: impl FnMut(char) -> bool) -> Option<&'a str> {
        let start = self.offset().to_usize();

        while !self.cursor.is_eof() {
            if end_predicate(self.cursor.first()) {
                return Some(&self.source[start..self.offset().to_usize()]);
            }
            self.cursor.bump();
        }

        None
    }

    pub(crate) fn parse(mut self) -> Result<ErrorAssertion<'a>, String> {
        let mut column = None;
        let mut rule = None;

        self.skip_whitespace();

        while !self.cursor.is_eof() {
            match self.cursor.first() {
                '0'..='9' if column.is_none() && rule.is_none() => {
                    let Some(column_str) = self.consume_until(char::is_whitespace) else {
                        return Err("no rule or message text".to_string());
                    };
                    column = OneIndexed::from_str(column_str)
                        .map_err(|_| format!("Bad column number `{column_str}`"))
                        .map(Some)?;
                }

                '[' if rule.is_none() => {
                    self.cursor.bump();
                    let Some(position) = memchr::memmem::find(self.cursor.as_bytes(), b"]") else {
                        return Err("expected ']' to close rule code".to_string());
                    };
                    rule = Some(&self.cursor.as_str()[..position]);
                    self.cursor.skip_bytes(position + b"]".len());
                }

                '"' => {
                    self.cursor.bump();
                    let rest =
                        &self.source[self.offset().to_usize()..self.source.trim_end().len() - 1];
                    self.cursor.skip_bytes(rest.len());
                    return if self.cursor.first() == '"' {
                        Ok(ErrorAssertion {
                            rule,
                            column,
                            message_contains: Some(rest),
                        })
                    } else {
                        Err("expected '\"' to close message".to_string())
                    };
                }

                '\n' | '\r' => break,

                unexpected => {
                    return Err(format!(
                        "encountered unexpected character `{unexpected}` at offset {offset} (relative to comment `#`)",
                        unexpected = unexpected,
                        offset = self.offset().to_usize() + ERROR_PREFIX.len()
                    ))
                }
            }

            self.skip_whitespace();
        }

        if rule.is_none() {
            return Err("no rule or message text".to_string());
        }

        Ok(ErrorAssertion {
            rule,
            column,
            message_contains: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_python_trivia::textwrap::dedent;
    use ruff_source_file::OneIndexed;

    fn get_assertions(source: &str) -> InlineFileAssertions {
        let mut db = crate::db::Db::setup(SystemPathBuf::from("/src"));
        db.write_file("/src/test.py", source).unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        InlineFileAssertions::from_file(&db, file)
    }

    fn as_vec(assertions: &InlineFileAssertions) -> Vec<LineAssertions> {
        assertions.into_iter().collect()
    }

    #[test]
    fn ty_display() {
        let assertions = get_assertions(&dedent(
            "
            reveal_type(1)  # revealed: Literal[1]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "revealed: Literal[1]");
    }

    #[test]
    fn error() {
        let assertions = get_assertions(&dedent(
            "
            x  # error:
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "error: ");
    }

    #[test]
    fn prior_line() {
        let assertions = get_assertions(&dedent(
            "
            # revealed: Literal[1]
            reveal_type(1)
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "revealed: Literal[1]");
    }

    #[test]
    fn stacked_prior_line() {
        let assertions = get_assertions(&dedent(
            "
            # revealed: Unbound
            # error: [unbound-name]
            reveal_type(x)
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(3));

        let [assert1, assert2] = &line.assertions[..] else {
            panic!("expected two assertions");
        };

        assert_eq!(format!("{assert1}"), "revealed: Unbound");
        assert_eq!(format!("{assert2}"), "error: [unbound-name]");
    }

    #[test]
    fn stacked_mixed() {
        let assertions = get_assertions(&dedent(
            "
            # revealed: Unbound
            reveal_type(x) # error: [unbound-name]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(2));

        let [assert1, assert2] = &line.assertions[..] else {
            panic!("expected two assertions");
        };

        assert_eq!(format!("{assert1}"), "revealed: Unbound");
        assert_eq!(format!("{assert2}"), "error: [unbound-name]");
    }

    #[test]
    fn multiple_lines() {
        let assertions = get_assertions(&dedent(
            r#"
            # error: [invalid-assignment]
            x: int = "foo"
            y  # error: [unbound-name]
            "#,
        ));

        let [line1, line2] = &as_vec(&assertions)[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line_number, OneIndexed::from_zero_indexed(2));
        assert_eq!(line2.line_number, OneIndexed::from_zero_indexed(3));

        let [UnparsedAssertion::Error(error1)] = &line1.assertions[..] else {
            panic!("expected one error assertion");
        };

        let error1 = ErrorAssertion::from_str(error1).unwrap();

        assert_eq!(error1.rule, Some("invalid-assignment"));

        let [UnparsedAssertion::Error(error2)] = &line2.assertions[..] else {
            panic!("expected one error assertion");
        };

        let error2 = ErrorAssertion::from_str(error2).unwrap();

        assert_eq!(error2.rule, Some("unbound-name"));
    }

    #[test]
    fn multiple_lines_mixed_stack() {
        let assertions = get_assertions(&dedent(
            r#"
            # error: [invalid-assignment]
            x: int = reveal_type("foo")  # revealed: str
            y  # error: [unbound-name]
            "#,
        ));

        let [line1, line2] = &as_vec(&assertions)[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line_number, OneIndexed::from_zero_indexed(2));
        assert_eq!(line2.line_number, OneIndexed::from_zero_indexed(3));

        let [UnparsedAssertion::Error(error1), UnparsedAssertion::Revealed(expected_ty)] =
            &line1.assertions[..]
        else {
            panic!("expected one error assertion and one Revealed assertion");
        };

        let error1 = ErrorAssertion::from_str(error1).unwrap();

        assert_eq!(error1.rule, Some("invalid-assignment"));
        assert_eq!(*expected_ty, "str");

        let [UnparsedAssertion::Error(error2)] = &line2.assertions[..] else {
            panic!("expected one error assertion");
        };

        let error2 = ErrorAssertion::from_str(error2).unwrap();

        assert_eq!(error2.rule, Some("unbound-name"));
    }

    #[test]
    fn error_with_rule() {
        let assertions = get_assertions(&dedent(
            "
            x  # error: [unbound-name]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "error: [unbound-name]");
    }

    #[test]
    fn error_with_rule_and_column() {
        let assertions = get_assertions(&dedent(
            "
            x  # error: 1 [unbound-name]
            ",
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(1));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), "error: 1 [unbound-name]");
    }

    #[test]
    fn error_with_rule_and_message() {
        let assertions = get_assertions(&dedent(
            r#"
            # error: [unbound-name] "`x` is unbound"
            x
            "#,
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(
            format!("{assert}"),
            r#"error: [unbound-name] "`x` is unbound""#
        );
    }

    #[test]
    fn error_with_message_and_column() {
        let assertions = get_assertions(&dedent(
            r#"
            # error: 1 "`x` is unbound"
            x
            "#,
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(format!("{assert}"), r#"error: 1 "`x` is unbound""#);
    }

    #[test]
    fn error_with_rule_and_message_and_column() {
        let assertions = get_assertions(&dedent(
            r#"
            # error: 1 [unbound-name] "`x` is unbound"
            x
            "#,
        ));

        let [line] = &as_vec(&assertions)[..] else {
            panic!("expected one line");
        };

        assert_eq!(line.line_number, OneIndexed::from_zero_indexed(2));

        let [assert] = &line.assertions[..] else {
            panic!("expected one assertion");
        };

        assert_eq!(
            format!("{assert}"),
            r#"error: 1 [unbound-name] "`x` is unbound""#
        );
    }
}
