use std::backtrace::BacktraceStatus;
use std::fmt::{Display, Write};
use std::path::{Component, PathBuf};
use std::sync::LazyLock;

use camino::Utf8Path;
use colored::Colorize;
use path_slash::{PathBufExt, PathExt};
use regex::Regex;
use similar::{ChangeTag, TextDiff};

use ruff_db::Db;
use ruff_db::diagnostic::{Diagnostic, DisplayDiagnosticConfig};
use ruff_db::files::File;
use ruff_db::panic::{PanicError, catch_unwind};
use ruff_db::source::line_index;
use ruff_db::system::SystemPath;
use ruff_diagnostics::Applicability;
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::{Ranged, TextRange};

use crate::matcher::Failure;
use crate::parser::{BacktickOffsets, EmbeddedFileSourceMap, MarkdownTest};

/// Filter which tests to run in mdtest.
///
/// Only tests whose names contain this filter string will be executed.
const MDTEST_TEST_FILTER: &str = "MDTEST_TEST_FILTER";

/// If set to a value other than "0", updates the content of inline snapshots.
const MDTEST_UPDATE_SNAPSHOTS: &str = "MDTEST_UPDATE_SNAPSHOTS";

/// Switch mdtest output format to GitHub Actions annotations.
///
/// If set (to any value), mdtest will output errors in GitHub Actions format.
const MDTEST_GITHUB_ANNOTATIONS_FORMAT: &str = "MDTEST_GITHUB_ANNOTATIONS_FORMAT";

mod assertion;
mod diagnostic;
pub mod matcher;
pub mod parser;

/// Options that customize how an mdtest suite is run.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// The rule expected by an otherwise unqualified `# error` assertion.
    pub default_error_rule: Option<&'static str>,
}

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
pub fn run<C>(
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    source: &str,
    test_name: &str,
    crate_name: &str,
    suite: &parser::MarkdownTestSuite<'_, C>,
    mut run_test: impl FnMut(
        &parser::MarkdownTest<'_, '_, C>,
        &mut String,
        OutputFormat,
    ) -> Result<(TestOutcome, Vec<MarkdownEdit>), Failures>,
) -> anyhow::Result<()> {
    let output_format = output_format();

    let mut markdown_edits = vec![];

    let filter = std::env::var(MDTEST_TEST_FILTER).ok();
    let mut any_failures = false;
    let mut assertion = String::new();
    for test in suite.tests() {
        if filter
            .as_ref()
            .is_some_and(|f| !(test.uncontracted_name().contains(f) || test.name() == *f))
        {
            continue;
        }

        // Buffer any immediate output from the test to emit after the header has been printed.
        let mut test_assertion = String::new();
        let result = run_test(&test, &mut test_assertion, output_format);

        let this_test_failed = result.is_err();
        any_failures = any_failures || this_test_failed;

        if this_test_failed && output_format.is_cli() {
            let _ = writeln!(assertion, "\n\n{}\n", test.name().bold().underline());
        }

        match result {
            Ok((_, edits)) => markdown_edits.extend(edits),
            Err(failures) => {
                let md_index = LineIndex::from_source_text(source);

                for test_failures in failures {
                    let source_map =
                        EmbeddedFileSourceMap::new(&md_index, test_failures.backtick_offsets);

                    for (relative_line_number, failures) in test_failures.by_line.iter() {
                        let file = relative_fixture_path.as_str();

                        let absolute_line_number =
                            match source_map.to_absolute_line_number(relative_line_number) {
                                Ok(line_number) => line_number,
                                Err(last_line_number) => {
                                    output_format.write_error(
                                        &mut assertion,
                                        file,
                                        last_line_number,
                                        &Failure::new(
                                            "Found a trailing assertion comment \
                                            (e.g., `# revealed:` or `# error:`) \
                                            not followed by any statement.",
                                        ),
                                    );

                                    continue;
                                }
                            };

                        for failure in failures {
                            output_format.write_error(
                                &mut assertion,
                                file,
                                absolute_line_number,
                                failure,
                            );
                        }
                    }
                }
            }
        }

        if this_test_failed && output_format.is_cli() {
            assertion.push_str(&test_assertion);

            let escaped_test_name = test.name().replace('\'', "\\'");
            let _ = writeln!(
                assertion,
                "\nTo rerun this specific test, \
                set the environment variable: {MDTEST_TEST_FILTER}='{escaped_test_name}'",
            );
            let _ = writeln!(
                assertion,
                "{MDTEST_TEST_FILTER}='{escaped_test_name}' cargo test -p {crate_name} \
                --test mdtest -- {test_name}",
            );

            let _ = writeln!(assertion, "\n{}", "-".repeat(50));
        }
    }

    if !markdown_edits.is_empty() {
        try_apply_markdown_edits(absolute_fixture_path, source, markdown_edits);
    }

    assert!(!any_failures, "{}", &assertion);

    Ok(())
}

/// Determine the output format from the `MDTEST_GITHUB_ANNOTATIONS_FORMAT` environment variable.
fn output_format() -> OutputFormat {
    if std::env::var(MDTEST_GITHUB_ANNOTATIONS_FORMAT).is_ok() {
        OutputFormat::GitHub
    } else {
        OutputFormat::Cli
    }
}

/// Defines the format in which mdtest should print an error to the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// The format `cargo test` should use by default.
    Cli,
    /// A format that will provide annotations from GitHub Actions
    /// if mdtest fails on a PR.
    /// See <https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions#setting-an-error-message>
    GitHub,
}

impl OutputFormat {
    const fn is_cli(self) -> bool {
        matches!(self, OutputFormat::Cli)
    }

    /// Write a test error in the appropriate format.
    ///
    /// For CLI format, errors are appended to `assertion_buf` so they appear
    /// in the assertion-failure message.
    ///
    /// For GitHub format, errors are printed directly to stdout so that GitHub
    /// Actions can detect them as workflow commands. Workflow commands must
    /// appear at the beginning of a line in stdout to be parsed by GitHub.
    #[expect(clippy::print_stdout)]
    fn write_error(
        self,
        assertion_buf: &mut String,
        file: &str,
        line: OneIndexed,
        failure: &Failure,
    ) {
        match self {
            OutputFormat::Cli => {
                let _ = writeln!(
                    assertion_buf,
                    "{file_line} {message}",
                    file_line = format!("{file}:{line}").cyan(),
                    message = Indented(failure.message()),
                );
                if let Some((expected, actual)) = failure.diff() {
                    let _ = render_diff(assertion_buf, actual, expected);
                }
            }
            OutputFormat::GitHub => {
                println!(
                    "::error file={file},line={line}::{message}",
                    message = failure.message()
                );
            }
        }
    }

    /// Write a module-resolution inconsistency in the appropriate format.
    ///
    /// See `write_error` for details on why GitHub-format
    /// messages must be printed directly to stdout.
    #[expect(clippy::print_stdout)]
    pub fn write_inconsistency(
        self,
        assertion_buf: &mut String,
        fixture_path: &Utf8Path,
        inconsistency: &impl Display,
    ) {
        match self {
            OutputFormat::Cli => {
                let info = fixture_path.to_string().cyan();
                let _ = writeln!(assertion_buf, "  {info} {inconsistency}");
            }
            OutputFormat::GitHub => {
                println!("::error file={fixture_path}::{inconsistency}");
            }
        }
    }
}

/// Indents every line except the first when formatting `T` by four spaces.
///
/// ## Examples
/// Wrapping the message part indents the `error[...]` diagnostic frame by four spaces:
///
/// ```text
/// crates/ty_python_semantic/resources/mdtest/mro.md:465 Fixing the diagnostics caused a fatal error:
///        error[internal-error]: Applying fixes introduced a syntax error. Reverting changes.
///        --> src/mdtest_snippet.py:1:1
///        info: This indicates a bug in ty.
/// ```
struct Indented<T>(T);

impl<T> std::fmt::Display for Indented<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut w = IndentingWriter {
            f,
            at_line_start: false,
        };
        write!(&mut w, "{}", self.0)
    }
}

struct IndentingWriter<'a, 'b> {
    f: &'a mut std::fmt::Formatter<'b>,
    at_line_start: bool,
}

impl Write for IndentingWriter<'_, '_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for part in s.split_inclusive('\n') {
            if self.at_line_start {
                self.f.write_str("    ")?;
            }
            self.f.write_str(part)?;
            self.at_line_start = part.ends_with('\n');
        }

        Ok(())
    }
}

pub type Failures = Vec<FileFailures>;

/// The failures for a single file in a test by line number.
pub struct FileFailures {
    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    pub backtick_offsets: Vec<BacktickOffsets>,

    /// The failures by lines in the file.
    pub by_line: matcher::FailuresByLine,
}

/// File in a test.
pub struct TestFile<'a> {
    pub file: ruff_db::files::File,

    /// Information about the checkable code block(s) that compose this file.
    pub code_blocks: Vec<parser::CodeBlock<'a>>,
}

impl TestFile<'_> {
    pub fn to_code_block_backtick_offsets(&self) -> Vec<BacktickOffsets> {
        self.code_blocks
            .iter()
            .map(parser::CodeBlock::backtick_offsets)
            .collect()
    }
}

fn diagnostic_display_config(tool_name: &'static str) -> DisplayDiagnosticConfig {
    DisplayDiagnosticConfig::new(tool_name)
        .color(false)
        .with_fix_applicability(Applicability::DisplayOnly)
        // Surrounding context in source annotations can be confusing in mdtests,
        // since you may get to see context from the *subsequent* code block (all
        // code blocks are merged into a single file). It also leads to a lot of
        // duplication in general. So we just set it to zero here for concise
        // and clear snapshots.
        .context(0)
}

pub fn render_diagnostic(db: &dyn Db, tool_name: &'static str, diagnostic: &Diagnostic) -> String {
    diagnostic
        .display(&db, &diagnostic_display_config(tool_name))
        .to_string()
}

fn render_diagnostics(db: &dyn Db, tool_name: &'static str, diagnostics: &[Diagnostic]) -> String {
    let mut rendered = String::new();
    for diag in diagnostics {
        writeln!(
            rendered,
            "{}",
            diag.display(&db, &diagnostic_display_config(tool_name))
        )
        .unwrap();
    }

    rendered.trim_end_matches('\n').to_string()
}

fn is_update_inline_snapshots_enabled() -> bool {
    let is_enabled: std::sync::LazyLock<_> = std::sync::LazyLock::new(|| {
        std::env::var_os(MDTEST_UPDATE_SNAPSHOTS).is_some_and(|v| v != "0")
    });
    *is_enabled
}

const SNAPSHOT_PATH_DELIMITERS: &str = "'\"`<>|:()[],;{}";

/// The database, tool name, and search roots used when rendering diagnostic snapshots.
///
/// Search roots help identify paths in snapshot text, including paths that do not exist.
pub struct SnapshotContext<'a> {
    db: &'a dyn Db,
    tool_name: &'static str,
    search_roots: &'a [&'a SystemPath],
}

impl<'a> SnapshotContext<'a> {
    pub const fn new(
        db: &'a dyn Db,
        tool_name: &'static str,
        search_roots: &'a [&'a SystemPath],
    ) -> Self {
        Self {
            db,
            tool_name,
            search_roots,
        }
    }

    /// Replaces backslashes in recognized paths with forward slashes in diagnostic snapshots.
    fn normalize_paths<'s>(&self, rendered: &'s str) -> std::borrow::Cow<'s, str> {
        // Find potential paths in diagnostic text as runs of characters containing a backslash,
        // bounded by whitespace or mdtest path delimiters. A drive prefix such as `C:` is allowed
        // at the start.
        static PATH: LazyLock<Regex> = LazyLock::new(|| {
            let delimiters = regex::escape(SNAPSHOT_PATH_DELIMITERS);
            Regex::new(&format!(
                r"(?:[A-Za-z]:)?[^\s{delimiters}]*\\[^\s{delimiters}]*"
            ))
            .unwrap()
        });

        PATH.replace_all(rendered, |captures: &regex::Captures<'_>| {
            let matched = &captures[0];
            // Trailing periods can be punctuation following a path in a diagnostic message. Exclude
            // them from the filesystem lookup and restore them after normalizing the path.
            let candidate = matched.trim_end_matches('.');

            // A lone backslash can be a Python line continuation.
            if candidate == "\\" {
                return matched.to_string();
            }

            // Interpret backslashes as path separators on the host platform before querying the
            // test filesystem. Leave the text unchanged if it cannot be represented as a system path.
            let path = PathBuf::from_backslash(candidate);
            let Some(system_path) = SystemPath::from_std_path(&path) else {
                return matched.to_string();
            };

            // Diagnostic paths can be relative to the current directory or a search root.
            let system = self.db.system();
            let exists = system.path_exists(system_path)
                || self
                    .search_roots
                    .iter()
                    .any(|root| system.path_exists(&root.join(system_path)))
                // `VendoredFileSystem::exists` panics for paths with a root or drive prefix.
                || (path.components().all(|component| {
                    matches!(
                        component,
                        Component::Normal(_) | Component::CurDir | Component::ParentDir
                    )
                }) && self.db.vendored().exists(system_path.as_str()));

            // For a candidate that doesn't exist, joining it to the current directory lets us check
            // whether it starts with a search root. If it does start with a search root, we only
            // normalize it if its components don't resemble Python escapes, such as `\x64` in the
            // source text.
            let absolute = system.current_directory().join(system_path);
            let under_root = self
                .search_roots
                .iter()
                .any(|root| absolute.starts_with(root));

            if exists || (under_root && !candidate.split('\\').skip(1).any(looks_like_escape)) {
                // `path-slash` can leave backslashes in Windows UNC prefixes; replace those too.
                let normalized = path.to_slash_lossy().replace('\\', "/");
                format!("{normalized}{}", &matched[candidate.len()..])
            } else {
                matched.to_string()
            }
        })
    }
}

/// Recognizes path components that start with text resembling a Python escape after a backslash.
fn looks_like_escape(component: &str) -> bool {
    static ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"^(?:x[0-9a-fA-F]{2}|u[0-9a-fA-F]{4}|U[0-9a-fA-F]{8}|N\{|[0-7]|[abfnrtv](?:$|["'`]))"#)
            .unwrap()
    });
    ESCAPE.is_match(component)
}

pub fn validate_inline_snapshot(
    context: &SnapshotContext<'_>,
    test_file: &TestFile<'_>,
    inline_diagnostics: &[Diagnostic],
    markdown_edits: &mut Vec<MarkdownEdit>,
    snapshot_filter: impl Fn(&str) -> String,
) -> Result<(), matcher::FailuresByLine> {
    let update_snapshots = is_update_inline_snapshots_enabled();
    let line_index = line_index(context.db, test_file.file);
    let mut failures = matcher::FailuresByLine::default();
    let mut inline_diagnostics = inline_diagnostics;

    // Group the inline diagnostics by code block. We do this by using the code blocks
    // start offsets. All diagnostics between the current's and next code blocks offset belong to the current code block.
    for (index, code_block) in test_file.code_blocks.iter().enumerate() {
        let next_block_start_offset = test_file
            .code_blocks
            .get(index + 1)
            .map_or(ruff_text_size::TextSize::new(u32::MAX), |next_code_block| {
                next_code_block.embedded_start_offset()
            });

        // Find the offset of the first diagnostic that belongs to the next code block.
        let diagnostics_end = inline_diagnostics
            .iter()
            .position(|diagnostic| {
                diagnostic
                    .primary_span()
                    .and_then(|span| span.range())
                    .map(TextRange::start)
                    .is_some_and(|offset| offset >= next_block_start_offset)
            })
            .unwrap_or(inline_diagnostics.len());

        let (block_diagnostics, remaining_diagnostics) =
            inline_diagnostics.split_at(diagnostics_end);
        inline_diagnostics = remaining_diagnostics;

        let failure_line = line_index.line_index(code_block.embedded_start_offset());

        let Some(first_diagnostic) = block_diagnostics.first() else {
            // If there are no inline diagnostics (no usages of `# snapshot`) but the code block has a
            // diagnostics section, mark it as unnecessary or remove it.
            if let Some(snapshot_code_block) = code_block.inline_snapshot_block() {
                if update_snapshots {
                    markdown_edits.push(MarkdownEdit {
                        range: snapshot_code_block.range(),
                        replacement: String::new(),
                    });
                } else {
                    failures.push(
                        failure_line,
                        vec![Failure::new(
                            "This code block has a `snapshot` code block but no `# snapshot` \
                            assertions. Remove the `snapshot` code block or add a `# snapshot:` \
                            assertion.",
                        )],
                    );
                }
            }

            continue;
        };

        let rendered = render_diagnostics(context.db, context.tool_name, block_diagnostics);
        let actual = snapshot_filter(&context.normalize_paths(&rendered));

        let Some(snapshot_code_block) = code_block.inline_snapshot_block() else {
            if update_snapshots {
                markdown_edits.push(MarkdownEdit {
                    range: TextRange::empty(code_block.backtick_offsets().end()),
                    replacement: format!("\n\n```snapshot\n{actual}\n```"),
                });
            } else {
                let first_range = first_diagnostic.primary_span().unwrap().range().unwrap();
                let line = line_index.line_index(first_range.start());
                failures.push(
                    line,
                    vec![Failure::new(format!(
                        "Add a `snapshot` block for this `# snapshot` assertion, \
                        or set `{MDTEST_UPDATE_SNAPSHOTS}=1` to insert one automatically",
                    ))],
                );
            }
            continue;
        };

        if snapshot_code_block.expected == actual {
            continue;
        }

        if update_snapshots {
            markdown_edits.push(MarkdownEdit {
                range: snapshot_code_block.range(),
                replacement: format!("```snapshot\n{actual}\n```"),
            });
        } else {
            failures.push(
                failure_line,
                vec![
                    Failure::new(format_args!(
                        "inline diagnostics snapshot are out of date; \
                        set `{MDTEST_UPDATE_SNAPSHOTS}=1` to update the `snapshot` block",
                    ))
                    .with_diff(snapshot_code_block.expected.to_string(), actual),
                ],
            );
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn render_diff(f: &mut dyn std::fmt::Write, expected: &str, actual: &str) -> std::fmt::Result {
    let diff = TextDiff::from_lines(expected, actual);

    writeln!(f, "{}", "--- expected".red())?;
    writeln!(f, "{}", "+++ actual".green())?;

    let mut unified = diff.unified_diff();
    let unified = unified.header("expected", "actual");

    for hunk in unified.iter_hunks() {
        writeln!(f, "{}", hunk.header())?;

        for change in hunk.iter_changes() {
            let value = change.value();
            match change.tag() {
                ChangeTag::Equal => write!(f, " {value}")?,
                ChangeTag::Delete => {
                    write!(f, "{}{}", "-".red(), value.red())?;
                }
                ChangeTag::Insert => {
                    write!(f, "{}{}", "+".green(), value.green()).unwrap();
                }
            }

            if !diff.newline_terminated() || change.missing_newline() {
                writeln!(f)?;
            }
        }
    }

    Ok(())
}

fn try_apply_markdown_edits(
    absolute_fixture_path: &Utf8Path,
    source: &str,
    mut edits: Vec<MarkdownEdit>,
) {
    edits.sort_unstable_by_key(|edit| edit.range.start());

    let mut updated = source.to_string();
    for edit in edits.into_iter().rev() {
        updated.replace_range(
            edit.range.start().to_usize()..edit.range.end().to_usize(),
            &edit.replacement,
        );
    }

    if let Err(err) = std::fs::write(absolute_fixture_path, updated) {
        tracing::error!("Failed to write updated inline snapshots in: {err}");
    }
}

fn create_diagnostic_snapshot<'d, C>(
    db: &dyn Db,
    tool_name: &'static str,
    relative_fixture_path: &Utf8Path,
    test: &parser::MarkdownTest<'_, '_, C>,
    diagnostics: impl IntoIterator<Item = &'d Diagnostic>,
) -> String {
    let mut snapshot = String::new();
    writeln!(snapshot).unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot, "mdtest name: {}", test.uncontracted_name()).unwrap();
    writeln!(
        snapshot,
        "mdtest path: {}",
        relative_fixture_path.as_std_path().to_slash_lossy()
    )
    .unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot).unwrap();

    let source_heading = if test
        .files()
        .all(|file| matches!(file.lang, "py" | "python" | "pyi" | "ipynb"))
    {
        "Python source files"
    } else {
        "Source files"
    };
    writeln!(snapshot, "# {source_heading}").unwrap();
    writeln!(snapshot).unwrap();
    for file in test.files() {
        writeln!(snapshot, "## {}", file.relative_path()).unwrap();
        writeln!(snapshot).unwrap();
        // Note that we don't use ```py here because the line numbering
        // we add makes it invalid Python. This sacrifices syntax
        // highlighting when you look at the snapshot on GitHub,
        // but the line numbers are extremely useful for analyzing
        // snapshots. So we keep them.
        writeln!(snapshot, "```").unwrap();

        let line_number_width = file.code.lines().count().to_string().len();
        for (i, line) in file.code.lines().enumerate() {
            let line_number = i + 1;
            writeln!(snapshot, "{line_number:>line_number_width$} | {line}").unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        writeln!(snapshot).unwrap();
    }

    writeln!(snapshot, "# Diagnostics").unwrap();
    writeln!(snapshot).unwrap();
    for (index, diagnostic) in diagnostics.into_iter().enumerate() {
        if index > 0 {
            writeln!(snapshot).unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        write!(
            snapshot,
            "{}",
            diagnostic.display(&db, &diagnostic_display_config(tool_name))
        )
        .unwrap();
        writeln!(snapshot, "```").unwrap();
    }
    snapshot
}

#[derive(Debug, Clone)]
pub struct MarkdownEdit {
    range: TextRange,
    replacement: String,
}

/// Run a function over an embedded test file, catching any panics that occur in the process.
///
/// If no panic occurs, the result of the function is returned as an `Ok()` variant.
///
/// If a panic occurs, a nicely formatted [`FileFailures`] is returned as an `Err()` variant to
/// be formatted into a diagnostic message by callers.
pub fn attempt_test<'a, T, F>(
    test_fn: F,
    test_file: &'a TestFile<'a>,
) -> Result<T, AttemptTestError<'a>>
where
    F: FnOnce(File) -> T + std::panic::UnwindSafe,
{
    catch_unwind(|| test_fn(test_file.file)).map_err(|info| AttemptTestError { info, test_file })
}

pub struct AttemptTestError<'a> {
    pub info: PanicError,
    test_file: &'a TestFile<'a>,
}

impl AttemptTestError<'_> {
    pub fn into_file_failures(
        self,
        db: &dyn Db,
        action: &str,
        clarification: Option<&str>,
    ) -> FileFailures {
        let info = self.info;

        let mut by_line = matcher::FailuresByLine::default();
        let mut messages = vec![];
        match info.location {
            Some(location) => messages.push(Failure::new(format_args!(
                "Attempting to {action} caused a panic at {location}"
            ))),
            None => messages.push(Failure::new(format_args!(
                "Attempting to {action} caused a panic at an unknown location",
            ))),
        }
        if let Some(clarification) = clarification {
            messages.push(Failure::new(clarification));
        }
        messages.push(Failure::new(""));
        messages.push(Failure::new(info.payload));
        messages.push(Failure::new(""));

        if let Some(backtrace) = info.backtrace {
            match backtrace.status() {
                BacktraceStatus::Disabled => {
                    let msg = "run with `RUST_BACKTRACE=1` environment variable to \
                         a backtrace";
                    messages.push(Failure::new(msg));
                }
                BacktraceStatus::Captured => {
                    messages.extend(backtrace.to_string().split('\n').map(Failure::new));
                }
                _ => {}
            }
        }

        if let Some(backtrace) = info.salsa_backtrace {
            salsa::attach(db, || {
                messages.extend(format!("{backtrace:#}").split('\n').map(Failure::new));
            });
        }

        by_line.push(OneIndexed::from_zero_indexed(0), messages);

        FileFailures {
            backtick_offsets: self.test_file.to_code_block_backtick_offsets(),
            by_line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestOutcome {
    Success,
    Skipped,
}

pub fn check_panic<C>(test: &MarkdownTest<'_, '_, C>, panic_info: Option<PanicError>) {
    match panic_info {
        Some(panic_info) => {
            let expected_message = test
                .should_expect_panic()
                .expect("panic_info is only set when `should_expect_panic` is `Ok`");

            if let Some(expected_message) = expected_message {
                let message = panic_info.payload.to_string();
                assert!(
                    message.contains(expected_message),
                    "Test `{}` is expected to panic with `{expected_message}`, \
                    but panicked with `{message}` instead.",
                    test.name(),
                );
            }
        }
        None => {
            if let Ok(message) = test.should_expect_panic() {
                if let Some(message) = message {
                    panic!(
                        "Test `{}` is expected to panic with `{message}`, but it didn't.",
                        test.name()
                    );
                }
                panic!("Test `{}` is expected to panic but it didn't.", test.name());
            }
        }
    }
}

pub fn snapshot_diagnostics<C>(
    test: &MarkdownTest<'_, '_, C>,
    context: &SnapshotContext<'_>,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    diagnostics: &[Diagnostic],
    mut snapshot_filter: impl FnMut(&Diagnostic) -> bool,
) {
    if test.should_snapshot_diagnostics() {
        assert!(
            !diagnostics.is_empty(),
            "Test `{}` requested snapshotting diagnostics but it didn't produce any.",
            test.name()
        );

        let snapshot = crate::create_diagnostic_snapshot(
            context.db,
            context.tool_name,
            relative_fixture_path,
            test,
            diagnostics
                .iter()
                .filter(|diagnostic| snapshot_filter(diagnostic)),
        );

        let name = test.name().replace(' ', "_").replace(':', "__");
        insta::with_settings!(
            {
                snapshot_path => snapshot_path,
                input_file => name.clone(),
                prepend_module_to_snapshot => false,
            },
            { insta::assert_snapshot!(name, context.normalize_paths(&snapshot)) }
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use ruff_db::Db;
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem, System, SystemPath, TestSystem};
    use ruff_db::vendored::{VendoredFileSystem, VendoredFileSystemBuilder};
    use zip::CompressionMethod;

    /// Database that can be used for testing.
    ///
    /// Uses an in-memory filesystem and an empty vendored filesystem. Since the
    /// parser only needs source text and line info, no typeshed stubs are required.
    #[salsa::db]
    #[derive(Default, Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
    }

    impl TestDb {
        pub(crate) fn setup() -> Self {
            Self::default()
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
    impl salsa::Database for TestDb {}

    /// Checks that snapshot paths use forward slashes while escape sequences are preserved.
    #[test]
    fn snapshot_paths_preserve_escape_sequences() -> anyhow::Result<()> {
        let mut db = TestDb::setup();
        let mut vendored = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
        vendored.add_directory("stdlib/")?;
        vendored.add_directory("stdlib/asyncio/")?;
        vendored.add_file("stdlib/builtins.pyi", "")?;
        vendored.add_file("stdlib/asyncio/tasks.pyi", "")?;
        db.vendored = vendored.finish()?;
        db.write_files([
            ("/src/nested/example.py", ""),
            ("/src/a/file.py", ""),
            ("/src/@extra/example.py", ""),
            ("/src/other+test/module.py", ""),
            ("/src/9876/file.py", ""),
            ("/src/script", ""),
            ("C:/src/nested/example.py", ""),
            ("C:/tools/Python/module.py", ""),
            ("C:/tools/a/file.py", ""),
            ("C:/a/file.py", ""),
            ("//server/share/module.py", ""),
        ])?;
        let search_roots = [SystemPath::new("/src")];
        let snapshot = r#"error[example]: Type `C @ src\nested\example.py:1:1` contains `Literal["prefix\x1b\u200b\U000e0001"]`
 --> C:\src\nested\example.py:1:9
  |
1 | value = "\x1b\u200b\n"
2 | format_spec = f"{value:\b}"
3 | format_spec = f"{x:\b}"
4 | zero = b"\0"
5 | name = "\N{LATIN SMALL LETTER A}"
6 | data = b"\x00abc\xff"
7 | more = b"\0abc\101hello\2024\x1b."
8 | simple = "\n,\t."
9 | dotted = "\n.foo"
10 | dotted_text = "prefix\n.foo"
11 | escaped_filename = "\x64.py"
12 | text_after_escape = "\nfoo"
13 | format_spec = f"{x!r:\b}"
14 | raw = r"\n\q"
15 | escaped_under_root = "src\x1b\u200b"
16 | escaped_prefix = "src\n"
17 | continuation = \
  |         ^^^^^^^^^^^^^^
help: Read C:\tools\Python\module.py
help: Read \\server\share\module.py, \src, and C:\a
help: See src\@extra\example.py, src\other+test\module.py, and src\9876
help: See nested\example.py and src\script
help: Inspect nested\example.py.
help: Expected src\missing\module.py and src\new\module.py
 ::: stdlib\builtins.pyi:1:1
 ::: stdlib\asyncio\tasks.pyi:1:1
help: Read stdlib\asyncio and stdlib\missing.pyi
help: Missing \unknown\module.py, C:\unknown\module.py, and C:unknown\module.py
info:   1. /src\a (first-party code)
info:   2. C:\tools\a (extra search path)
"#;
        let expected = r#"error[example]: Type `C @ src/nested/example.py:1:1` contains `Literal["prefix\x1b\u200b\U000e0001"]`
 --> C:/src/nested/example.py:1:9
  |
1 | value = "\x1b\u200b\n"
2 | format_spec = f"{value:\b}"
3 | format_spec = f"{x:\b}"
4 | zero = b"\0"
5 | name = "\N{LATIN SMALL LETTER A}"
6 | data = b"\x00abc\xff"
7 | more = b"\0abc\101hello\2024\x1b."
8 | simple = "\n,\t."
9 | dotted = "\n.foo"
10 | dotted_text = "prefix\n.foo"
11 | escaped_filename = "\x64.py"
12 | text_after_escape = "\nfoo"
13 | format_spec = f"{x!r:\b}"
14 | raw = r"\n\q"
15 | escaped_under_root = "src\x1b\u200b"
16 | escaped_prefix = "src\n"
17 | continuation = \
  |         ^^^^^^^^^^^^^^
help: Read C:/tools/Python/module.py
help: Read //server/share/module.py, /src, and C:/a
help: See src/@extra/example.py, src/other+test/module.py, and src/9876
help: See nested/example.py and src/script
help: Inspect nested/example.py.
help: Expected src/missing/module.py and src/new/module.py
 ::: stdlib/builtins.pyi:1:1
 ::: stdlib/asyncio/tasks.pyi:1:1
help: Read stdlib/asyncio and stdlib\missing.pyi
help: Missing \unknown\module.py, C:\unknown\module.py, and C:unknown\module.py
info:   1. /src/a (first-party code)
info:   2. C:/tools/a (extra search path)
"#;

        assert_eq!(
            super::SnapshotContext::new(&db, "ty", &search_roots).normalize_paths(snapshot),
            expected
        );
        Ok(())
    }
}
