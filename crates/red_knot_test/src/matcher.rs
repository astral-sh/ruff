//! Match [`Diagnostic`]s against assertions and produce test failure
//! messages for any mismatches.
use crate::assertion::{InlineFileAssertions, ParsedAssertion, UnparsedAssertion};
use crate::db::Db;
use crate::diagnostic::SortedDiagnostics;
use colored::Colorize;
use ruff_db::diagnostic::{Diagnostic, DiagnosticAsStrError, DiagnosticId};
use ruff_db::files::File;
use ruff_db::source::{line_index, source_text, SourceText};
use ruff_source_file::{LineIndex, OneIndexed};
use std::cmp::Ordering;
use std::ops::Range;

#[derive(Debug, Default)]
pub(super) struct FailuresByLine {
    failures: Vec<String>,
    lines: Vec<LineFailures>,
}

impl FailuresByLine {
    pub(super) fn iter(&self) -> impl Iterator<Item = (OneIndexed, &[String])> {
        self.lines.iter().map(|line_failures| {
            (
                line_failures.line_number,
                &self.failures[line_failures.range.clone()],
            )
        })
    }

    pub(super) fn push(&mut self, line_number: OneIndexed, messages: Vec<String>) {
        let start = self.failures.len();
        self.failures.extend(messages);
        self.lines.push(LineFailures {
            line_number,
            range: start..self.failures.len(),
        });
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[derive(Debug)]
struct LineFailures {
    line_number: OneIndexed,
    range: Range<usize>,
}

pub(super) fn match_file(
    db: &Db,
    file: File,
    diagnostics: &[Diagnostic],
) -> Result<(), FailuresByLine> {
    // Parse assertions from comments in the file, and get diagnostics from the file; both
    // ordered by line number.
    let assertions = InlineFileAssertions::from_file(db, file);
    let diagnostics = SortedDiagnostics::new(diagnostics, &line_index(db, file));

    // Get iterators over assertions and diagnostics grouped by line, in ascending line order.
    let mut line_assertions = assertions.into_iter();
    let mut line_diagnostics = diagnostics.iter_lines();

    let mut current_assertions = line_assertions.next();
    let mut current_diagnostics = line_diagnostics.next();

    let matcher = Matcher::from_file(db, file);
    let mut failures = FailuresByLine::default();

    loop {
        match (&current_assertions, &current_diagnostics) {
            (Some(assertions), Some(diagnostics)) => {
                match assertions.line_number.cmp(&diagnostics.line_number) {
                    Ordering::Equal => {
                        // We have assertions and diagnostics on the same line; check for
                        // matches and error on any that don't match, then advance both
                        // iterators.
                        matcher
                            .match_line(diagnostics, assertions)
                            .unwrap_or_else(|messages| {
                                failures.push(assertions.line_number, messages);
                            });
                        current_assertions = line_assertions.next();
                        current_diagnostics = line_diagnostics.next();
                    }
                    Ordering::Less => {
                        // We have assertions on an earlier line than diagnostics; report these
                        // assertions as all unmatched, and advance the assertions iterator.
                        failures.push(assertions.line_number, unmatched(assertions));
                        current_assertions = line_assertions.next();
                    }
                    Ordering::Greater => {
                        // We have diagnostics on an earlier line than assertions; report these
                        // diagnostics as all unmatched, and advance the diagnostics iterator.
                        failures.push(diagnostics.line_number, unmatched(diagnostics));
                        current_diagnostics = line_diagnostics.next();
                    }
                }
            }
            (Some(assertions), None) => {
                // We've exhausted diagnostics but still have assertions; report these assertions
                // as unmatched and advance the assertions iterator.
                failures.push(assertions.line_number, unmatched(assertions));
                current_assertions = line_assertions.next();
            }
            (None, Some(diagnostics)) => {
                // We've exhausted assertions but still have diagnostics; report these
                // diagnostics as unmatched and advance the diagnostics iterator.
                failures.push(diagnostics.line_number, unmatched(diagnostics));
                current_diagnostics = line_diagnostics.next();
            }
            // When we've exhausted both diagnostics and assertions, break.
            (None, None) => break,
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

trait Unmatched {
    fn unmatched(&self) -> String;
}

fn unmatched<'a, T: Unmatched + 'a>(unmatched: &'a [T]) -> Vec<String> {
    unmatched.iter().map(Unmatched::unmatched).collect()
}

trait UnmatchedWithColumn {
    fn unmatched_with_column(&self, column: OneIndexed) -> String;
}

// This is necessary since we only parse assertions lazily,
// and sometimes we know before parsing any assertions that an assertion will be unmatched,
// e.g. if we've exhausted all diagnostics but there are still assertions left.
//
// TODO: the lazy parsing means that we sometimes won't report malformed assertions as
// being invalid if we detect that they'll be unmatched before parsing them.
// That's perhaps not the best user experience.
impl Unmatched for UnparsedAssertion<'_> {
    fn unmatched(&self) -> String {
        format!("{} {self}", "unmatched assertion:".red())
    }
}

impl Unmatched for ParsedAssertion<'_> {
    fn unmatched(&self) -> String {
        format!("{} {self}", "unmatched assertion:".red())
    }
}

fn maybe_add_undefined_reveal_clarification(
    diagnostic: &Diagnostic,
    original: std::fmt::Arguments,
) -> String {
    if diagnostic.id().is_lint_named("undefined-reveal") {
        format!(
            "{} add a `# revealed` assertion on this line (original diagnostic: {original})",
            "used built-in `reveal_type`:".yellow()
        )
    } else {
        format!("{} {original}", "unexpected error:".red())
    }
}

impl Unmatched for &Diagnostic {
    fn unmatched(&self) -> String {
        let id = self.id();
        let id = id.as_str().unwrap_or_else(|error| match error {
            DiagnosticAsStrError::Category { name, .. } => name,
        });

        maybe_add_undefined_reveal_clarification(
            self,
            format_args!(r#"[{id}] "{message}""#, message = self.primary_message()),
        )
    }
}

impl UnmatchedWithColumn for &Diagnostic {
    fn unmatched_with_column(&self, column: OneIndexed) -> String {
        let id = self.id();
        let id = id.as_str().unwrap_or_else(|error| match error {
            DiagnosticAsStrError::Category { name, .. } => name,
        });

        maybe_add_undefined_reveal_clarification(
            self,
            format_args!(
                r#"{column} [{id}] "{message}""#,
                message = self.primary_message()
            ),
        )
    }
}

/// Discard `@Todo`-type metadata from expected types, which is not available
/// when running in release mode.
#[cfg(not(debug_assertions))]
fn discard_todo_metadata(ty: &str) -> std::borrow::Cow<'_, str> {
    static TODO_METADATA_REGEX: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"@Todo\([^)]*\)").unwrap());

    TODO_METADATA_REGEX.replace_all(ty, "@Todo")
}

struct Matcher {
    line_index: LineIndex,
    source: SourceText,
}

impl Matcher {
    fn from_file(db: &Db, file: File) -> Self {
        Self {
            line_index: line_index(db, file),
            source: source_text(db, file),
        }
    }

    /// Check a slice of [`Diagnostic`]s against a slice of
    /// [`UnparsedAssertion`]s.
    ///
    /// Return vector of [`Unmatched`] for any unmatched diagnostics or
    /// assertions.
    fn match_line<'a, 'b>(
        &self,
        diagnostics: &'a [&'a Diagnostic],
        assertions: &'a [UnparsedAssertion<'b>],
    ) -> Result<(), Vec<String>>
    where
        'b: 'a,
    {
        let mut failures = vec![];
        let mut unmatched = diagnostics.to_vec();
        for assertion in assertions {
            match assertion.parse() {
                Ok(assertion) => {
                    if !self.matches(&assertion, &mut unmatched) {
                        failures.push(assertion.unmatched());
                    }
                }
                Err(error) => {
                    failures.push(format!("{} {}", "invalid assertion:".red(), error));
                }
            }
        }
        for diagnostic in unmatched {
            failures.push(diagnostic.unmatched_with_column(self.column(diagnostic)));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures)
        }
    }

    fn column(&self, diagnostic: &Diagnostic) -> OneIndexed {
        diagnostic
            .primary_span()
            .and_then(|span| span.range())
            .map(|range| {
                self.line_index
                    .source_location(range.start(), &self.source)
                    .column
            })
            .unwrap_or(OneIndexed::from_zero_indexed(0))
    }

    /// Check if `assertion` matches any [`Diagnostic`]s in `unmatched`.
    ///
    /// If so, return `true` and remove the matched diagnostics from `unmatched`. Otherwise, return
    /// `false`.
    ///
    /// An `Error` assertion can only match one diagnostic; even if it could match more than one,
    /// we short-circuit after the first match.
    ///
    /// A `Revealed` assertion must match a revealed-type diagnostic, and may also match an
    /// undefined-reveal diagnostic, if present.
    fn matches(&self, assertion: &ParsedAssertion, unmatched: &mut Vec<&Diagnostic>) -> bool {
        match assertion {
            ParsedAssertion::Error(error) => {
                let position = unmatched.iter().position(|diagnostic| {
                    !error.rule.is_some_and(|rule| {
                        !(diagnostic.id().is_lint_named(rule) || diagnostic.id().matches(rule))
                    }) && error
                        .column
                        .is_none_or(|col| col == self.column(diagnostic))
                        && error
                            .message_contains
                            .is_none_or(|needle| diagnostic.primary_message().contains(needle))
                });
                if let Some(position) = position {
                    unmatched.swap_remove(position);
                    true
                } else {
                    false
                }
            }
            ParsedAssertion::Revealed(expected_type) => {
                #[cfg(not(debug_assertions))]
                let expected_type = discard_todo_metadata(&expected_type);

                let mut matched_revealed_type = None;
                let mut matched_undefined_reveal = None;
                let expected_reveal_type_message = format!("Revealed type is `{expected_type}`");
                for (index, diagnostic) in unmatched.iter().enumerate() {
                    if matched_revealed_type.is_none()
                        && diagnostic.id() == DiagnosticId::RevealedType
                        && diagnostic.primary_message() == expected_reveal_type_message
                    {
                        matched_revealed_type = Some(index);
                    } else if matched_undefined_reveal.is_none()
                        && diagnostic.id().is_lint_named("undefined-reveal")
                    {
                        matched_undefined_reveal = Some(index);
                    }
                    if matched_revealed_type.is_some() && matched_undefined_reveal.is_some() {
                        break;
                    }
                }
                let mut idx = 0;
                unmatched.retain(|_| {
                    let retain =
                        Some(idx) != matched_revealed_type && Some(idx) != matched_undefined_reveal;
                    idx += 1;
                    retain
                });
                matched_revealed_type.is_some()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FailuresByLine;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, Severity, Span};
    use ruff_db::files::{system_path_to_file, File};
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_source_file::OneIndexed;
    use ruff_text_size::TextRange;

    struct ExpectedDiagnostic {
        id: DiagnosticId,
        message: &'static str,
        range: TextRange,
    }

    impl ExpectedDiagnostic {
        fn new(id: DiagnosticId, message: &'static str, offset: usize) -> Self {
            let offset: u32 = offset.try_into().unwrap();
            Self {
                id,
                message,
                range: TextRange::new(offset.into(), (offset + 1).into()),
            }
        }

        fn into_diagnostic(self, file: File) -> Diagnostic {
            let mut diag = Diagnostic::new(self.id, Severity::Error, "");
            let span = Span::from(file).with_range(self.range);
            diag.annotate(Annotation::primary(span).message(self.message));
            diag
        }
    }

    fn get_result(
        source: &str,
        expected_diagnostics: Vec<ExpectedDiagnostic>,
    ) -> Result<(), FailuresByLine> {
        colored::control::set_override(false);

        let mut db = crate::db::Db::setup();
        db.write_file("/src/test.py", source).unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();

        let diagnostics: Vec<Diagnostic> = expected_diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.into_diagnostic(file))
            .collect();
        super::match_file(&db, file, &diagnostics)
    }

    fn assert_fail(result: Result<(), FailuresByLine>, messages: &[(usize, &[&str])]) {
        let Err(failures) = result else {
            panic!("expected a failure");
        };

        let expected: Vec<(OneIndexed, Vec<String>)> = messages
            .iter()
            .map(|(idx, msgs)| {
                (
                    OneIndexed::from_zero_indexed(*idx),
                    msgs.iter().map(ToString::to_string).collect(),
                )
            })
            .collect();
        let failures: Vec<(OneIndexed, Vec<String>)> = failures
            .iter()
            .map(|(idx, msgs)| (idx, msgs.to_vec()))
            .collect();

        assert_eq!(failures, expected);
    }

    fn assert_ok(result: &Result<(), FailuresByLine>) {
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn revealed_match() {
        let result = get_result(
            "x # revealed: Foo",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::RevealedType,
                "Revealed type is `Foo`",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn revealed_wrong_rule() {
        let result = get_result(
            "x # revealed: Foo",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("not-revealed-type"),
                "Revealed type is `Foo`",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "unmatched assertion: revealed: Foo",
                    r#"unexpected error: 1 [not-revealed-type] "Revealed type is `Foo`""#,
                ],
            )],
        );
    }

    #[test]
    fn revealed_wrong_message() {
        let result = get_result(
            "x # revealed: Foo",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::RevealedType,
                "Something else",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "unmatched assertion: revealed: Foo",
                    r#"unexpected error: 1 [revealed-type] "Something else""#,
                ],
            )],
        );
    }

    #[test]
    fn revealed_unmatched() {
        let result = get_result("x # revealed: Foo", vec![]);

        assert_fail(result, &[(0, &["unmatched assertion: revealed: Foo"])]);
    }

    #[test]
    fn revealed_match_with_undefined() {
        let result = get_result(
            "x # revealed: Foo",
            vec![
                ExpectedDiagnostic::new(DiagnosticId::RevealedType, "Revealed type is `Foo`", 0),
                ExpectedDiagnostic::new(
                    DiagnosticId::lint("undefined-reveal"),
                    "Doesn't matter",
                    0,
                ),
            ],
        );

        assert_ok(&result);
    }

    #[test]
    fn revealed_match_with_only_undefined() {
        let result = get_result(
            "x # revealed: Foo",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("undefined-reveal"),
                "Doesn't matter",
                0,
            )],
        );

        assert_fail(result, &[(0, &["unmatched assertion: revealed: Foo"])]);
    }

    #[test]
    fn revealed_mismatch_with_undefined() {
        let result = get_result(
            "x # revealed: Foo",
            vec![
                ExpectedDiagnostic::new(DiagnosticId::RevealedType, "Revealed type is `Bar`", 0),
                ExpectedDiagnostic::new(
                    DiagnosticId::lint("undefined-reveal"),
                    "Doesn't matter",
                    0,
                ),
            ],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "unmatched assertion: revealed: Foo",
                    r#"unexpected error: 1 [revealed-type] "Revealed type is `Bar`""#,
                ],
            )],
        );
    }

    #[test]
    fn undefined_reveal_type_unmatched() {
        let result = get_result(
            "reveal_type(1)",
            vec![
                ExpectedDiagnostic::new(
                    DiagnosticId::lint("undefined-reveal"),
                    "undefined reveal message",
                    0,
                ),
                ExpectedDiagnostic::new(
                    DiagnosticId::RevealedType,
                    "Revealed type is `Literal[1]`",
                    12,
                ),
            ],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "used built-in `reveal_type`: add a `# revealed` assertion on this line (\
                    original diagnostic: [undefined-reveal] \"undefined reveal message\")",
                    r#"unexpected error: [revealed-type] "Revealed type is `Literal[1]`""#,
                ],
            )],
        );
    }

    #[test]
    fn undefined_reveal_type_mismatched() {
        let result = get_result(
            "reveal_type(1) # error: [something-else]",
            vec![
                ExpectedDiagnostic::new(
                    DiagnosticId::lint("undefined-reveal"),
                    "undefined reveal message",
                    0,
                ),
                ExpectedDiagnostic::new(
                    DiagnosticId::RevealedType,
                    "Revealed type is `Literal[1]`",
                    12,
                ),
            ],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "unmatched assertion: error: [something-else]",
                    "used built-in `reveal_type`: add a `# revealed` assertion on this line (\
                    original diagnostic: 1 [undefined-reveal] \"undefined reveal message\")",
                    r#"unexpected error: 13 [revealed-type] "Revealed type is `Literal[1]`""#,
                ],
            )],
        );
    }

    #[test]
    fn error_unmatched() {
        let result = get_result("x # error: [rule]", vec![]);

        assert_fail(result, &[(0, &["unmatched assertion: error: [rule]"])]);
    }

    #[test]
    fn error_match_rule() {
        let result = get_result(
            "x # error: [some-rule]",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "Any message",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_match_rule_no_whitespace() {
        let result = get_result(
            "x #error:[some-rule]",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "Any message",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_match_rule_lots_of_whitespace() {
        let result = get_result(
            "x   #  error  :  [ some-rule ]",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "Any message",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_wrong_rule() {
        let result = get_result(
            "x # error: [some-rule]",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("anything"),
                "Any message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "unmatched assertion: error: [some-rule]",
                    r#"unexpected error: 1 [anything] "Any message""#,
                ],
            )],
        );
    }

    #[test]
    fn error_match_message() {
        let result = get_result(
            r#"x # error: "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("anything"),
                "message contains this",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_wrong_message() {
        let result = get_result(
            r#"x # error: "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("anything"),
                "Any message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    r#"unmatched assertion: error: "contains this""#,
                    r#"unexpected error: 1 [anything] "Any message""#,
                ],
            )],
        );
    }

    #[test]
    fn error_match_column_and_rule() {
        let result = get_result(
            "x # error: 1 [some-rule]",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "Any message",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_match_column_and_rule_and_message() {
        let result = get_result(
            r#"x # error: 5 [some-rule] "Some message""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "Some message",
                4,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_wrong_column() {
        let result = get_result(
            "x # error: 2 [rule]",
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("rule"),
                "Any message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "unmatched assertion: error: 2 [rule]",
                    r#"unexpected error: 1 [rule] "Any message""#,
                ],
            )],
        );
    }

    #[test]
    fn error_match_column_and_message() {
        let result = get_result(
            r#"x # error: 1 "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("anything"),
                "message contains this",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_match_rule_and_message() {
        let result = get_result(
            r#"x # error: [a-rule] "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("a-rule"),
                "message contains this",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_match_all() {
        let result = get_result(
            r#"x # error: 1 [a-rule] "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("a-rule"),
                "message contains this",
                0,
            )],
        );

        assert_ok(&result);
    }

    #[test]
    fn error_match_all_wrong_column() {
        let result = get_result(
            r#"x # error: 2 [some-rule] "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "message contains this",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    r#"unmatched assertion: error: 2 [some-rule] "contains this""#,
                    r#"unexpected error: 1 [some-rule] "message contains this""#,
                ],
            )],
        );
    }

    #[test]
    fn error_match_all_wrong_rule() {
        let result = get_result(
            r#"x # error: 1 [some-rule] "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("other-rule"),
                "message contains this",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    r#"unmatched assertion: error: 1 [some-rule] "contains this""#,
                    r#"unexpected error: 1 [other-rule] "message contains this""#,
                ],
            )],
        );
    }

    #[test]
    fn error_match_all_wrong_message() {
        let result = get_result(
            r#"x # error: 1 [some-rule] "contains this""#,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "Any message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    r#"unmatched assertion: error: 1 [some-rule] "contains this""#,
                    r#"unexpected error: 1 [some-rule] "Any message""#,
                ],
            )],
        );
    }

    #[test]
    fn interspersed_matches_and_mismatches() {
        let source = dedent(
            r#"
            1 # error: [line-one]
            2
            3 # error: [line-three]
            4 # error: [line-four]
            5
            6: # error: [line-six]
            "#,
        );
        let two = source.find('2').unwrap();
        let three = source.find('3').unwrap();
        let five = source.find('5').unwrap();
        let result = get_result(
            &source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("line-two"), "msg", two),
                ExpectedDiagnostic::new(DiagnosticId::lint("line-three"), "msg", three),
                ExpectedDiagnostic::new(DiagnosticId::lint("line-five"), "msg", five),
            ],
        );

        assert_fail(
            result,
            &[
                (1, &["unmatched assertion: error: [line-one]"]),
                (2, &[r#"unexpected error: [line-two] "msg""#]),
                (4, &["unmatched assertion: error: [line-four]"]),
                (5, &[r#"unexpected error: [line-five] "msg""#]),
                (6, &["unmatched assertion: error: [line-six]"]),
            ],
        );
    }

    #[test]
    fn more_diagnostics_than_assertions() {
        let source = dedent(
            r#"
            1 # error: [line-one]
            2
            "#,
        );
        let one = source.find('1').unwrap();
        let two = source.find('2').unwrap();
        let result = get_result(
            &source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("line-one"), "msg", one),
                ExpectedDiagnostic::new(DiagnosticId::lint("line-two"), "msg", two),
            ],
        );

        assert_fail(result, &[(2, &[r#"unexpected error: [line-two] "msg""#])]);
    }

    #[test]
    fn multiple_assertions_and_diagnostics_same_line() {
        let source = dedent(
            "
            # error: [one-rule]
            # error: [other-rule]
            x
            ",
        );
        let x = source.find('x').unwrap();
        let result = get_result(
            &source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("one-rule"), "msg", x),
                ExpectedDiagnostic::new(DiagnosticId::lint("other-rule"), "msg", x),
            ],
        );

        assert_ok(&result);
    }

    #[test]
    fn multiple_assertions_and_diagnostics_same_line_all_same() {
        let source = dedent(
            "
            # error: [one-rule]
            # error: [one-rule]
            x
            ",
        );
        let x = source.find('x').unwrap();
        let result = get_result(
            &source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("one-rule"), "msg", x),
                ExpectedDiagnostic::new(DiagnosticId::lint("one-rule"), "msg", x),
            ],
        );

        assert_ok(&result);
    }

    #[test]
    fn multiple_assertions_and_diagnostics_same_line_mismatch() {
        let source = dedent(
            "
            # error: [one-rule]
            # error: [other-rule]
            x
            ",
        );
        let x = source.find('x').unwrap();
        let result = get_result(
            &source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("one-rule"), "msg", x),
                ExpectedDiagnostic::new(DiagnosticId::lint("other-rule"), "msg", x),
                ExpectedDiagnostic::new(DiagnosticId::lint("third-rule"), "msg", x),
            ],
        );

        assert_fail(
            result,
            &[(3, &[r#"unexpected error: 1 [third-rule] "msg""#])],
        );
    }

    #[test]
    fn parenthesized_expression() {
        let source = dedent(
            "
            a = b + (
                error: [undefined-reveal]
                reveal_type(5)  # revealed: Literal[5]
            )
            ",
        );
        let reveal = source.find("reveal_type").unwrap();
        let result = get_result(
            &source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("undefined-reveal"), "msg", reveal),
                ExpectedDiagnostic::new(
                    DiagnosticId::RevealedType,
                    "Revealed type is `Literal[5]`",
                    reveal,
                ),
            ],
        );

        assert_ok(&result);
    }

    #[test]
    fn bare_error_assertion_not_allowed() {
        let source = "x  # error:";
        let x = source.find('x').unwrap();
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                x,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: no rule or message text",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn bare_reveal_assertion_not_allowed() {
        let source = "x  # revealed: ";
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: Must specify which type should be revealed",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn column_only_error_assertion_not_allowed() {
        let source = "x  # error: 1";
        let x = source.find('x').unwrap();
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                x,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: no rule or message text",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn unclosed_rule_not_allowed() {
        let source = r#"x  # error: 42 [some-rule "Some message""#;
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: expected ']' to close rule code",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn bad_column_number_not_allowed() {
        let source = r#"x  # error: 3.14 [some-rule] "Some message""#;
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: bad column number `3.14`",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn multiple_column_numbers_not_allowed() {
        let source = r#"x  # error: 3 14 [some-rule] "Some message""#;
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: multiple column numbers in one assertion",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn multiple_column_numbers_not_allowed_even_if_interspersed() {
        let source = r#"x  # error: 3 [some-rule] 14 "Some message""#;
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: multiple column numbers in one assertion",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn two_rule_codes_not_allowed() {
        let source = r#"x  # error: [rule1] [rule2] "Some message""#;
        let result = get_result(
            source,
            vec![
                ExpectedDiagnostic::new(DiagnosticId::lint("rule1"), "Some message", 0),
                ExpectedDiagnostic::new(DiagnosticId::lint("rule2"), "Some message", 0),
            ],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: cannot use multiple rule codes in one assertion",
                    r#"unexpected error: 1 [rule1] "Some message""#,
                    r#"unexpected error: 1 [rule2] "Some message""#,
                ],
            )],
        );
    }

    #[test]
    fn column_number_not_allowed_after_rule_code() {
        let source = r#"x  # error: [rule1] 4 "Some message""#;
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("rule1"),
                "Some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: column number must precede the rule code",
                    r#"unexpected error: 1 [rule1] "Some message""#,
                ],
            )],
        );
    }

    #[test]
    fn column_number_not_allowed_after_message() {
        let source = r#"x  # error: "Some message" 0"#;
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("rule1"),
                "Some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: expected '\"' to be the final character in an assertion with an error message",
                    r#"unexpected error: 1 [rule1] "Some message""#,
                ],
            )],
        );
    }

    #[test]
    fn unclosed_message_not_allowed() {
        let source = "x  # error: \"Some message";
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: expected '\"' to be the final character in an assertion with an error message",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }

    #[test]
    fn unclosed_message_not_allowed_even_after_rule_code() {
        let source = "x  # error: [some-rule] \"Some message";
        let result = get_result(
            source,
            vec![ExpectedDiagnostic::new(
                DiagnosticId::lint("some-rule"),
                "some message",
                0,
            )],
        );

        assert_fail(
            result,
            &[(
                0,
                &[
                    "invalid assertion: expected '\"' to be the final character in an assertion with an error message",
                    r#"unexpected error: 1 [some-rule] "some message""#,
                ],
            )],
        );
    }
}
