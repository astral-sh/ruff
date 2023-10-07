use std::fmt::{Display, Formatter};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use itertools::Itertools;
use log::error;
use rayon::iter::Either::{Left, Right};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use tracing::debug;

use ruff_diagnostics::SourceMap;
use ruff_linter::fs;
use ruff_linter::logging::LogLevel;
use ruff_linter::source_kind::{SourceError, SourceKind};
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_python_formatter::{format_module_source, FormatModuleError};
use ruff_text_size::{TextLen, TextRange, TextSize};
use ruff_workspace::resolver::python_files_in_path;
use ruff_workspace::FormatterSettings;

use crate::args::{CliOverrides, FormatArguments};
use crate::panic::{catch_unwind, PanicError};
use crate::resolve::resolve;
use crate::ExitStatus;

#[derive(Debug, Copy, Clone, is_macro::Is)]
pub(crate) enum FormatMode {
    /// Write the formatted contents back to the file.
    Write,
    /// Check if the file is formatted, but do not write the formatted contents back.
    Check,
}

/// Format a set of files, and return the exit status.
pub(crate) fn format(
    cli: &FormatArguments,
    overrides: &CliOverrides,
    log_level: LogLevel,
) -> Result<ExitStatus> {
    let pyproject_config = resolve(
        cli.isolated,
        cli.config.as_deref(),
        overrides,
        cli.stdin_filename.as_deref(),
    )?;
    let mode = if cli.check {
        FormatMode::Check
    } else {
        FormatMode::Write
    };
    let (paths, resolver) = python_files_in_path(&cli.files, &pyproject_config, overrides)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(ExitStatus::Success);
    }

    let start = Instant::now();
    let (results, errors): (Vec<_>, Vec<_>) = paths
        .into_par_iter()
        .filter_map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.into_path();

                    let SourceType::Python(source_type) = SourceType::from(&path) else {
                        // Ignore any non-Python files.
                        return None;
                    };

                    let resolved_settings = resolver.resolve(&path, &pyproject_config);

                    Some(
                        match catch_unwind(|| {
                            format_path(&path, &resolved_settings.formatter, source_type, mode)
                        }) {
                            Ok(inner) => inner.map(|result| FormatPathResult { path, result }),
                            Err(error) => Err(FormatCommandError::Panic(Some(path), error)),
                        },
                    )
                }
                Err(err) => Some(Err(FormatCommandError::Ignore(err))),
            }
        })
        .partition_map(|result| match result {
            Ok(diagnostic) => Left(diagnostic),
            Err(err) => Right(err),
        });
    let duration = start.elapsed();

    debug!(
        "Formatted {} files in {:.2?}",
        results.len() + errors.len(),
        duration
    );

    // Report on any errors.
    for error in &errors {
        error!("{error}");
    }

    let summary = FormatSummary::new(results.as_slice(), mode);

    // Report on the formatting changes.
    if log_level >= LogLevel::Default {
        #[allow(clippy::print_stdout)]
        {
            println!("{summary}");
        }
    }

    match mode {
        FormatMode::Write => {
            if errors.is_empty() {
                Ok(ExitStatus::Success)
            } else {
                Ok(ExitStatus::Error)
            }
        }
        FormatMode::Check => {
            if errors.is_empty() {
                if summary.any_formatted() {
                    Ok(ExitStatus::Failure)
                } else {
                    Ok(ExitStatus::Success)
                }
            } else {
                Ok(ExitStatus::Error)
            }
        }
    }
}

/// Format the file at the given [`Path`].
#[tracing::instrument(skip_all, fields(path = %path.display()))]
fn format_path(
    path: &Path,
    settings: &FormatterSettings,
    source_type: PySourceType,
    mode: FormatMode,
) -> Result<FormatResult, FormatCommandError> {
    // Extract the sources from the file.
    let source_kind = match SourceKind::from_path(path, source_type) {
        Ok(Some(source_kind)) => source_kind,
        Ok(None) => return Ok(FormatResult::Unchanged),
        Err(err) => {
            return Err(FormatCommandError::Read(Some(path.to_path_buf()), err));
        }
    };

    // Format the source.
    match format_source(source_kind, source_type, Some(path), settings)? {
        FormattedSource::Formatted(formatted) => {
            if mode.is_write() {
                let mut writer = File::create(path).map_err(|err| {
                    FormatCommandError::Write(Some(path.to_path_buf()), err.into())
                })?;
                formatted
                    .write(&mut writer)
                    .map_err(|err| FormatCommandError::Write(Some(path.to_path_buf()), err))?;
            }
            Ok(FormatResult::Formatted)
        }
        FormattedSource::Unchanged(_) => Ok(FormatResult::Unchanged),
    }
}

#[derive(Debug)]
pub(crate) enum FormattedSource {
    /// The source was formatted, and the [`SourceKind`] contains the transformed source code.
    Formatted(SourceKind),
    /// The source was unchanged, and the [`SourceKind`] contains the original source code.
    Unchanged(SourceKind),
}

impl From<FormattedSource> for FormatResult {
    fn from(value: FormattedSource) -> Self {
        match value {
            FormattedSource::Formatted(_) => FormatResult::Formatted,
            FormattedSource::Unchanged(_) => FormatResult::Unchanged,
        }
    }
}

impl FormattedSource {
    pub(crate) fn source_kind(&self) -> &SourceKind {
        match self {
            FormattedSource::Formatted(source_kind) => source_kind,
            FormattedSource::Unchanged(source_kind) => source_kind,
        }
    }
}

/// Format a [`SourceKind`], returning the transformed [`SourceKind`], or `None` if the source was
/// unchanged.
pub(crate) fn format_source(
    source_kind: SourceKind,
    source_type: PySourceType,
    path: Option<&Path>,
    settings: &FormatterSettings,
) -> Result<FormattedSource, FormatCommandError> {
    match source_kind {
        SourceKind::Python(unformatted) => {
            let options = settings.to_format_options(source_type, &unformatted);

            let formatted = format_module_source(&unformatted, options)
                .map_err(|err| FormatCommandError::Format(path.map(Path::to_path_buf), err))?;

            let formatted = formatted.into_code();
            if formatted.len() == unformatted.len() && formatted == *unformatted {
                Ok(FormattedSource::Unchanged(SourceKind::Python(unformatted)))
            } else {
                Ok(FormattedSource::Formatted(SourceKind::Python(formatted)))
            }
        }
        SourceKind::IpyNotebook(notebook) => {
            if !notebook.is_python_notebook() {
                return Ok(FormattedSource::Unchanged(SourceKind::IpyNotebook(
                    notebook,
                )));
            }

            let options = settings.to_format_options(source_type, notebook.source_code());

            let mut output: Option<String> = None;
            let mut last: Option<TextSize> = None;
            let mut source_map = SourceMap::default();

            // Format each cell individually.
            for (start, end) in notebook.cell_offsets().iter().tuple_windows::<(_, _)>() {
                let range = TextRange::new(*start, *end);
                let unformatted = &notebook.source_code()[range];

                // Format the cell.
                let formatted = format_module_source(unformatted, options.clone())
                    .map_err(|err| FormatCommandError::Format(path.map(Path::to_path_buf), err))?;

                // If the cell is unchanged, skip it.
                let formatted = formatted.as_code();
                if formatted.len() == unformatted.len() && formatted == unformatted {
                    continue;
                }

                // If this is the first newly-formatted cell, initialize the output.
                let output = output
                    .get_or_insert_with(|| String::with_capacity(notebook.source_code().len()));

                // Add all contents from `last` to the current cell.
                let slice = &notebook.source_code()
                    [TextRange::new(last.unwrap_or_default(), range.start())];
                output.push_str(slice);

                // Add the start source marker for the cell.
                source_map.push_marker(*start, output.text_len());

                // Add the cell itself.
                output.push_str(formatted);

                // Add the end source marker for the added cell.
                source_map.push_marker(*end, output.text_len());

                // Track that the cell was formatted.
                last = Some(*end);
            }

            // If the file was unchanged, return `None`.
            let (Some(mut output), Some(last)) = (output, last) else {
                return Ok(FormattedSource::Unchanged(SourceKind::IpyNotebook(
                    notebook,
                )));
            };

            // Add the remaining content.
            let slice = &notebook.source_code()[usize::from(last)..];
            output.push_str(slice);

            // Update the notebook.
            let mut notebook = notebook.clone();
            notebook.update(&source_map, output);

            Ok(FormattedSource::Formatted(SourceKind::IpyNotebook(
                notebook,
            )))
        }
    }
}

/// The result of an individual formatting operation.
#[derive(Debug, Clone, Copy, is_macro::Is)]
pub(crate) enum FormatResult {
    /// The file was formatted.
    Formatted,
    /// The file was unchanged, as the formatted contents matched the existing contents.
    Unchanged,
}

/// The coupling of a [`FormatResult`] with the path of the file that was analyzed.
#[derive(Debug)]
struct FormatPathResult {
    path: PathBuf,
    result: FormatResult,
}

/// A summary of the formatting results.
#[derive(Debug)]
struct FormatSummary<'a> {
    /// The individual formatting results.
    results: &'a [FormatPathResult],
    /// The format mode that was used.
    mode: FormatMode,
}

impl<'a> FormatSummary<'a> {
    fn new(results: &'a [FormatPathResult], mode: FormatMode) -> Self {
        Self { results, mode }
    }

    /// Returns `true` if any of the files require formatting.
    fn any_formatted(&self) -> bool {
        self.results
            .iter()
            .any(|result| result.result.is_formatted())
    }
}

impl Display for FormatSummary<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Compute the number of changed and unchanged files.
        let mut formatted = 0u32;
        let mut unchanged = 0u32;
        for result in self.results {
            match result.result {
                FormatResult::Formatted => {
                    // If we're running in check mode, report on any files that would be formatted.
                    if self.mode.is_check() {
                        writeln!(
                            f,
                            "Would reformat: {}",
                            fs::relativize_path(&result.path).bold()
                        )?;
                    }
                    formatted += 1;
                }
                FormatResult::Unchanged => unchanged += 1,
            }
        }

        // Write out a summary of the formatting results.
        if formatted > 0 && unchanged > 0 {
            write!(
                f,
                "{} file{} {}, {} file{} left unchanged",
                formatted,
                if formatted == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check => "would be reformatted",
                },
                unchanged,
                if unchanged == 1 { "" } else { "s" },
            )
        } else if formatted > 0 {
            write!(
                f,
                "{} file{} {}",
                formatted,
                if formatted == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check => "would be reformatted",
                }
            )
        } else if unchanged > 0 {
            write!(
                f,
                "{} file{} left unchanged",
                unchanged,
                if unchanged == 1 { "" } else { "s" },
            )
        } else {
            Ok(())
        }
    }
}

/// An error that can occur while formatting a set of files.
#[derive(Error, Debug)]
pub(crate) enum FormatCommandError {
    Ignore(#[from] ignore::Error),
    Panic(Option<PathBuf>, PanicError),
    Read(Option<PathBuf>, SourceError),
    Format(Option<PathBuf>, FormatModuleError),
    Write(Option<PathBuf>, SourceError),
}

impl Display for FormatCommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ignore(err) => {
                if let ignore::Error::WithPath { path, .. } = err {
                    write!(
                        f,
                        "{}{}{} {}",
                        "Failed to format ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold(),
                        err.io_error()
                            .map_or_else(|| err.to_string(), std::string::ToString::to_string)
                    )
                } else {
                    write!(
                        f,
                        "{} {}",
                        "Encountered error:".bold(),
                        err.io_error()
                            .map_or_else(|| err.to_string(), std::string::ToString::to_string)
                    )
                }
            }
            Self::Read(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to read ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(f, "{}{} {err}", "Failed to read".bold(), ":".bold())
                }
            }
            Self::Write(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to write ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(f, "{}{} {err}", "Failed to write".bold(), ":".bold())
                }
            }
            Self::Format(path, err) => {
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {err}",
                        "Failed to format ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(f, "{}{} {err}", "Failed to format".bold(), ":".bold())
                }
            }
            Self::Panic(path, err) => {
                let message = r#"This indicates a bug in Ruff. If you could open an issue at:

    https://github.com/astral-sh/ruff/issues/new?title=%5BFormatter%20panic%5D

...with the relevant file contents, the `pyproject.toml` settings, and the following stack trace, we'd be very appreciative!
"#;
                if let Some(path) = path {
                    write!(
                        f,
                        "{}{}{} {message}\n{err}",
                        "Panicked while formatting ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    )
                } else {
                    write!(
                        f,
                        "{} {message}\n{err}",
                        "Panicked while formatting.".bold()
                    )
                }
            }
        }
    }
}
