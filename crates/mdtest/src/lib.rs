use camino::Utf8Path;
use colored::Colorize;
use ruff_db::diagnostic::{Diagnostic, DisplayDiagnosticConfig, FileResolver};
use ruff_diagnostics::Applicability;
use ruff_source_file::OneIndexed;
use std::fmt::{Display, Write};

use parser::BacktickOffsets;

mod assertion;
pub mod config;
pub mod db;
mod diagnostic;
pub mod matcher;
pub mod parser;

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
    pub const fn is_cli(self) -> bool {
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
    pub fn write_error(
        self,
        assertion_buf: &mut String,
        file: &str,
        line: OneIndexed,
        failure: impl Display,
    ) {
        match self {
            OutputFormat::Cli => {
                let _ = writeln!(
                    assertion_buf,
                    "  {file_line} {failure}",
                    file_line = format!("{file}:{line}").cyan()
                );
            }
            OutputFormat::GitHub => {
                println!("::error file={file},line={line}::{failure}");
            }
        }
    }

    /// Write a module-resolution inconsistency in the appropriate format.
    ///
    /// See [`write_error`](Self::write_error) for details on why GitHub-format
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

pub type Failures = Vec<FileFailures>;

/// The failures for a single file in a test by line number.
pub struct FileFailures {
    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    pub backtick_offsets: Vec<BacktickOffsets>,

    /// The failures by lines in the file.
    pub by_line: matcher::FailuresByLine,
}

/// File in a test.
pub struct TestFile {
    pub file: ruff_db::files::File,

    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    pub backtick_offsets: Vec<BacktickOffsets>,
}

pub fn create_diagnostic_snapshot<C>(
    resolver: &dyn FileResolver,
    tool_name: &'static str,
    relative_fixture_path: &Utf8Path,
    test: &parser::MarkdownTest<'_, '_, C>,
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> String {
    let display_config = DisplayDiagnosticConfig::new(tool_name)
        .color(false)
        .show_fix_diff(true)
        .with_fix_applicability(Applicability::DisplayOnly);

    let mut snapshot = String::new();
    writeln!(snapshot).unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot, "mdtest name: {}", test.uncontracted_name()).unwrap();
    writeln!(snapshot, "mdtest path: {relative_fixture_path}").unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot).unwrap();

    writeln!(snapshot, "# Python source files").unwrap();
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
    for (i, diag) in diagnostics.into_iter().enumerate() {
        if i > 0 {
            writeln!(snapshot).unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        write!(snapshot, "{}", diag.display(resolver, &display_config)).unwrap();
        writeln!(snapshot, "```").unwrap();
    }
    snapshot
}
