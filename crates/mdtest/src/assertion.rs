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

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast::token::Token;
use ruff_python_trivia::{CommentRanges, Cursor};
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::{Ranged, TextRange, TextSize};
use smallvec::SmallVec;
use std::str::FromStr;

/// Diagnostic assertion comments in a single embedded file.
#[derive(Debug)]
pub(crate) struct InlineFileAssertions<'s> {
    by_line: Vec<LineAssertions<'s>>,
}

impl<'s> InlineFileAssertions<'s> {
    pub(crate) fn from_file(
        source: &'s str,
        parsed: &ParsedModuleRef,
        file_index: &LineIndex,
    ) -> Self {
        let mut by_line = Vec::new();
        let mut file_assertions = UnparsedAssertionsIter {
            tokens: parsed.tokens().iter(),
            source,
        }
        .peekable();

        while let Some(ranged_assertion) = file_assertions.next() {
            let mut collector = AssertionVec::new();
            let mut line_number = file_index.line_index(ranged_assertion.start());

            // Collect all own-line comments on consecutive lines; these all apply to the same line of
            // code. For example:
            //
            // ```py
            // # error: [unbound-name]
            // # revealed: Unbound
            // reveal_type(x)
            // ```
            //
            if CommentRanges::is_own_line(ranged_assertion.start(), source) {
                collector.push(ranged_assertion.into_comment());
                let mut only_own_line = true;

                while let Some(ranged_assertion) = file_assertions.next_if(|next_pragma| {
                    let next_line_number = line_number.saturating_add(1);

                    if file_index.line_index(next_pragma.start()) == next_line_number {
                        line_number = next_line_number;
                        true
                    } else {
                        false
                    }
                }) {
                    if !CommentRanges::is_own_line(ranged_assertion.start(), source) {
                        only_own_line = false;
                    }

                    collector.push(ranged_assertion.into_comment());

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
                }

                if only_own_line {
                    // The collected comments apply to the _next_ line in the code.
                    line_number = line_number.saturating_add(1);
                }
            } else {
                // We have a line-trailing comment; it applies to its own line, and is not grouped.
                collector.push(ranged_assertion.into_comment());
            }

            by_line.push(LineAssertions {
                line_number,
                assertions: collector,
            });
        }

        Self { by_line }
    }
}

impl<'a, 's> IntoIterator for &'a InlineFileAssertions<'s> {
    type Item = &'a LineAssertions<'s>;

    type IntoIter = std::slice::Iter<'a, LineAssertions<'s>>;

    fn into_iter(self) -> Self::IntoIter {
        self.by_line.iter()
    }
}

impl<'s> IntoIterator for InlineFileAssertions<'s> {
    type Item = LineAssertions<'s>;

    type IntoIter = std::vec::IntoIter<LineAssertions<'s>>;

    fn into_iter(self) -> Self::IntoIter {
        self.by_line.into_iter()
    }
}

struct UnparsedAssertionsIter<'a, 's> {
    source: &'s str,
    tokens: std::slice::Iter<'a, Token>,
}

impl<'s> Iterator for UnparsedAssertionsIter<'_, 's> {
    type Item = AssertionWithRange<'s>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let token = self.tokens.next()?;
            if !token.kind().is_comment() {
                continue;
            }

            let comment_text = &self.source[token.range()];
            if let Some(assertion) = UnparsedAssertion::from_comment(comment_text) {
                return Some(AssertionWithRange(assertion, token.range()));
            }
        }
    }
}

/// An [`UnparsedAssertion`] with the [`TextRange`] of its original inline comment.
#[derive(Debug)]
struct AssertionWithRange<'a>(UnparsedAssertion<'a>, TextRange);

impl<'a> AssertionWithRange<'a> {
    fn into_comment(self) -> UnparsedAssertion<'a> {
        self.0
    }
}

impl Ranged for AssertionWithRange<'_> {
    fn range(&self) -> TextRange {
        self.1
    }
}

/// A vector of [`UnparsedAssertion`]s belonging to a single line.
///
/// Most lines will have zero or one assertion, so we use a [`SmallVec`] optimized for a single
/// element to avoid most heap vector allocations.
type AssertionVec<'a> = SmallVec<[UnparsedAssertion<'a>; 1]>;

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

/// A single assertion comment.
///
/// This type represents an *attempted* assertion, but not necessarily a *valid* assertion.
/// Parsing is done lazily in `matcher.rs`; this allows us to emit nicer error messages
/// in the event of an invalid assertion.
#[derive(Debug)]
pub(crate) enum UnparsedAssertion<'a> {
    /// A `# revealed:` assertion.
    Revealed(&'a str),
    /// An `# error:` assertion.
    Error(&'a str),
    /// A `# snapshot` assertion
    Snapshot(&'a str),
}

impl std::fmt::Display for UnparsedAssertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Revealed(expected_type) => {
                write!(f, "revealed: {expected_type}")
            }
            Self::Error(assertion) => write!(f, "error: {assertion}"),
            Self::Snapshot(assertion) => write!(f, "snapshot: {assertion}"),
        }
    }
}

impl<'a> UnparsedAssertion<'a> {
    /// Returns `Some(_)` if the comment starts with `# error`, `# snapshot`, or `# revealed`,
    /// indicating that it is a assertion comment.
    fn from_comment(comment: &'a str) -> Option<Self> {
        let comment = comment.trim().strip_prefix('#')?.trim();

        // Support other pragma comments coming after `error` or `revealed`, e.g.
        // `# error: [code] # type: ignore` (nested pragma comments)
        let comment = if let Some((before_nested, _)) = comment.split_once('#') {
            before_nested
        } else {
            comment
        };

        let (keyword, body) = comment.split_once(':').unwrap_or((comment, ""));

        let keyword = keyword.trim();
        let body = body.trim();

        match keyword {
            "revealed" => Some(Self::Revealed(body)),
            "error" => Some(Self::Error(body)),
            "snapshot" => Some(Self::Snapshot(body)),
            _ => None,
        }
    }

    /// Parse the attempted assertion into a [`ParsedAssertion`] structured representation.
    pub(crate) fn parse(&self) -> Result<ParsedAssertion<'a>, PragmaParseError<'a>> {
        match self {
            Self::Revealed(revealed) => {
                if revealed.is_empty() {
                    Err(PragmaParseError::EmptyRevealTypeAssertion)
                } else {
                    Ok(ParsedAssertion::Revealed(revealed))
                }
            }
            Self::Error(error) => ErrorAssertion::from_str(error)
                .map(ParsedAssertion::Error)
                .map_err(PragmaParseError::ErrorAssertionParseError),
            Self::Snapshot(rule) => {
                if rule.is_empty() {
                    Ok(ParsedAssertion::Snapshot(None))
                } else {
                    Ok(ParsedAssertion::Snapshot(Some(rule)))
                }
            }
        }
    }
}

/// An assertion comment that has been parsed and validated for correctness.
#[derive(Debug)]
pub(crate) enum ParsedAssertion<'a> {
    /// A `# revealed:` assertion.
    Revealed(&'a str),

    /// An `# error:` assertion.
    Error(ErrorAssertion<'a>),

    /// A `# snapshot: <code?>` assertion.
    Snapshot(Option<&'a str>),
}

impl std::fmt::Display for ParsedAssertion<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Revealed(expected_type) => write!(f, "revealed: {expected_type}"),
            Self::Error(assertion) => assertion.fmt(f),
            Self::Snapshot(rule) => match rule {
                Some(code) => write!(f, "snapshot: {code}"),
                None => write!(f, "snapshot"),
            },
        }
    }
}

/// A parsed and validated `# error:` assertion comment.
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
    fn from_str(source: &'a str) -> Result<Self, ErrorAssertionParseError<'a>> {
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

/// A parser to convert a string into a [`ErrorAssertion`].
#[derive(Debug, Clone)]
struct ErrorAssertionParser<'a> {
    cursor: Cursor<'a>,

    /// string slice representing all characters *after* the `# error:` prefix.
    comment_source: &'a str,
}

impl<'a> ErrorAssertionParser<'a> {
    fn new(comment: &'a str) -> Self {
        Self {
            cursor: Cursor::new(comment),
            comment_source: comment,
        }
    }

    /// Consume characters in the assertion comment until we find a non-whitespace character
    fn skip_whitespace(&mut self) {
        self.cursor.eat_while(char::is_whitespace);
    }

    /// Attempt to parse the assertion comment into a [`ErrorAssertion`].
    fn parse(mut self) -> Result<ErrorAssertion<'a>, ErrorAssertionParseError<'a>> {
        let mut column = None;
        let mut rule = None;

        self.skip_whitespace();

        while let Some(character) = self.cursor.bump() {
            match character {
                // column number
                '0'..='9' => {
                    if column.is_some() {
                        return Err(ErrorAssertionParseError::MultipleColumnNumbers);
                    }
                    if rule.is_some() {
                        return Err(ErrorAssertionParseError::ColumnNumberAfterRuleCode);
                    }
                    let offset = self.cursor.offset() - TextSize::new(1);
                    self.cursor.eat_while(|c| !c.is_whitespace());
                    let column_str =
                        &self.comment_source[TextRange::new(offset, self.cursor.offset())];
                    column = OneIndexed::from_str(column_str)
                        .map(Some)
                        .map_err(|e| ErrorAssertionParseError::BadColumnNumber(column_str, e))?;
                }

                // rule code
                '[' => {
                    if rule.is_some() {
                        return Err(ErrorAssertionParseError::MultipleRuleCodes);
                    }
                    let offset = self.cursor.offset();
                    self.cursor.eat_while(|c| c != ']');
                    if self.cursor.is_eof() {
                        return Err(ErrorAssertionParseError::UnclosedRuleCode);
                    }
                    rule = Some(
                        self.comment_source[TextRange::new(offset, self.cursor.offset())].trim(),
                    );
                    self.cursor.bump();
                }

                // message text
                '"' => {
                    let comment_source = self.comment_source.trim_end();
                    return if comment_source.ends_with('"') {
                        let start = self.cursor.offset().to_usize();
                        let end = comment_source.len() - 1;
                        if start > end {
                            return Err(ErrorAssertionParseError::DanglingMessageQuote);
                        }
                        let rest = &comment_source[start..end];
                        Ok(ErrorAssertion {
                            rule,
                            column,
                            message_contains: Some(rest),
                        })
                    } else {
                        Err(ErrorAssertionParseError::UnclosedMessage)
                    };
                }

                // Some other assumptions we make don't hold true if we hit this branch:
                '\n' | '\r' => {
                    unreachable!("Assertion comments should never contain newlines")
                }

                // something else (bad!)...
                unexpected => {
                    return Err(ErrorAssertionParseError::UnexpectedCharacter {
                        character: unexpected,
                        offset: self.cursor.offset().to_usize(),
                    });
                }
            }

            self.skip_whitespace();
        }

        if rule.is_some() {
            Ok(ErrorAssertion {
                rule,
                column,
                message_contains: None,
            })
        } else {
            Err(ErrorAssertionParseError::NoRuleOrMessage)
        }
    }
}

/// Enumeration of ways in which parsing an assertion comment can fail.
///
/// The assertion comment could be either a "revealed" assertion or an "error" assertion.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PragmaParseError<'a> {
    #[error("Must specify which type should be revealed")]
    EmptyRevealTypeAssertion,
    #[error("{0}")]
    ErrorAssertionParseError(ErrorAssertionParseError<'a>),
}

/// Enumeration of ways in which parsing an *error* assertion comment can fail.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ErrorAssertionParseError<'a> {
    #[error("no rule or message text")]
    NoRuleOrMessage,
    #[error("bad column number `{0}`")]
    BadColumnNumber(&'a str, #[source] std::num::ParseIntError),
    #[error("column number must precede the rule code")]
    ColumnNumberAfterRuleCode,
    #[error("multiple column numbers in one assertion")]
    MultipleColumnNumbers,
    #[error("expected ']' to close rule code")]
    UnclosedRuleCode,
    #[error("cannot use multiple rule codes in one assertion")]
    MultipleRuleCodes,
    #[error("expected '\"' to be the final character in an assertion with an error message")]
    UnclosedMessage,
    #[error("expected message text and closing '\"' after opening '\"'")]
    DanglingMessageQuote,
    #[error(
        "unexpected character `{character}` at offset {offset} (relative to the `:` in the assertion comment)"
    )]
    UnexpectedCharacter { character: char, offset: usize },
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use ruff_db::Db;
    use ruff_db::files::system_path_to_file;
    use ruff_db::files::{File, Files};
    use ruff_db::parsed::parsed_module;
    use ruff_db::source::line_index;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_source_file::OneIndexed;
    use ty_module_resolver::{SearchPathSettings, SearchPaths};
    use ty_python_core::platform::PythonPlatform;
    use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
    use ty_python_semantic::PythonVersionWithSource;

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    /// Database that can be used for testing.
    ///
    /// Uses an in memory filesystem and it stubs out the vendored files by default.
    #[salsa::db]
    #[derive(Default, Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        #[allow(unused)]
        events: Events,
    }

    impl TestDb {
        pub(crate) fn setup() -> Self {
            let events = Events::default();
            Self {
                storage: salsa::Storage::new(Some(Box::new({
                    let events = events.clone();
                    move |event| {
                        tracing::trace!("event: {:?}", event);
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored: ty_vendored::file_system().clone(),
                events,
                files: Files::default(),
            }
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }

        fn python_version(&self) -> ruff_python_ast::PythonVersion {
            ruff_python_ast::PythonVersion::latest_ty()
        }
    }

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    #[salsa::db]
    impl ty_module_resolver::Db for TestDb {
        fn search_paths(&self) -> &SearchPaths {
            Program::get(self).search_paths(self)
        }
    }

    #[salsa::db]
    impl ty_python_core::Db for TestDb {
        fn should_check_file(&self, file: File) -> bool {
            !file.path(self).is_vendored_path()
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    fn get_assertions(source: &str) -> InlineFileAssertions<'_> {
        let mut db = TestDb::setup();

        let settings = ProgramSettings {
            python_version: PythonVersionWithSource::default(),
            python_platform: PythonPlatform::default(),
            search_paths: SearchPathSettings::new(Vec::new())
                .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
                .unwrap(),
        };
        Program::init_or_update(&mut db, settings);

        db.write_file("/src/test.py", source).unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        InlineFileAssertions::from_file(source, &parsed, &line_index(&db, file))
    }

    fn into_vec(assertions: InlineFileAssertions<'_>) -> Vec<LineAssertions<'_>> {
        assertions.by_line
    }

    #[test]
    fn ty_display() {
        let source = dedent(
            "
            reveal_type(1)  # revealed: Literal[1]
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            "
            x  # error:
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            "
            # revealed: Literal[1]
            reveal_type(1)
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            "
            # revealed: Unbound
            # error: [unbound-name]
            reveal_type(x)
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            "
            # revealed: Unbound
            reveal_type(x) # error: [unbound-name]
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            r#"
            # error: [invalid-assignment]
            x: int = "foo"
            y  # error: [unbound-name]
            "#,
        );
        let assertions = get_assertions(&source);

        let [line1, line2] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            r#"
            # error: [invalid-assignment]
            x: int = reveal_type("foo")  # revealed: str
            y  # error: [unbound-name]
            "#,
        );
        let assertions = get_assertions(&source);

        let [line1, line2] = &into_vec(assertions)[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line_number, OneIndexed::from_zero_indexed(2));
        assert_eq!(line2.line_number, OneIndexed::from_zero_indexed(3));

        let [
            UnparsedAssertion::Error(error1),
            UnparsedAssertion::Revealed(expected_ty),
        ] = &line1.assertions[..]
        else {
            panic!("expected one error assertion and one Revealed assertion");
        };

        let error1 = ErrorAssertion::from_str(error1).unwrap();

        assert_eq!(error1.rule, Some("invalid-assignment"));
        assert_eq!(expected_ty.trim(), "str");

        let [UnparsedAssertion::Error(error2)] = &line2.assertions[..] else {
            panic!("expected one error assertion");
        };

        let error2 = ErrorAssertion::from_str(error2).unwrap();

        assert_eq!(error2.rule, Some("unbound-name"));
    }

    #[test]
    fn error_with_rule() {
        let source = dedent(
            "
            x  # error: [unbound-name]
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            "
            x  # error: 1 [unbound-name]
            ",
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            r#"
            # error: [unbound-name] "`x` is unbound"
            x
            "#,
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            r#"
            # error: 1 "`x` is unbound"
            x
            "#,
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
        let source = dedent(
            r#"
            # error: 1 [unbound-name] "`x` is unbound"
            x
            "#,
        );
        let assertions = get_assertions(&source);

        let [line] = &into_vec(assertions)[..] else {
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
