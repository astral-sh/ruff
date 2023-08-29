use std::fs::File;
use std::io;
use std::io::{BufWriter, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use log::{debug, warn};
use rayon::iter::Either::{Left, Right};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use tracing::{span, Level};

use ruff::fs;
use ruff::logging::LogLevel;
use ruff::warn_user_once;
use ruff_formatter::LineWidth;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_python_formatter::{format_module, FormatModuleError, PyFormatOptions};
use ruff_workspace::resolver::python_files_in_path;

use crate::args::{FormatArguments, Overrides};
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
    overrides: &Overrides,
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
        .map(|entry| {
            let entry = entry?;
            let path = entry.path();
            if let SourceType::Python(source_type @ (PySourceType::Python | PySourceType::Stub)) =
                SourceType::from(path)
            {
                let line_length = resolver.resolve(path, &pyproject_config).line_length;
                let options = PyFormatOptions::from_source_type(source_type)
                    .with_line_width(LineWidth::from(NonZeroU16::from(line_length)));
                format_path(path, options, mode)
            } else {
                Ok(FormatResult::Skipped)
            }
        })
        .partition_map(|result| match result {
            Ok(diagnostic) => Left(diagnostic),
            Err(err) => Right(err),
        });
    let duration = start.elapsed();
    debug!("Formatted files in: {:?}", duration);

    let summary = FormatResultSummary::from(results);

    // Report on any errors.
    if !errors.is_empty() {
        warn!("Encountered {} errors while formatting:", errors.len());
        for error in &errors {
            error.show_user();
        }
    }

    // Report on the formatting changes.
    if log_level >= LogLevel::Default {
        let mut writer: Box<dyn Write> = match &cli.output_file {
            Some(path) => {
                colored::control::set_override(false);
                let file = File::create(path)?;
                Box::new(BufWriter::new(file))
            }
            _ => Box::new(BufWriter::new(io::stdout())),
        };
        summary.show_user(&mut writer, mode)?;
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
                if summary.formatted > 0 {
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

#[tracing::instrument(skip_all, fields(path = %path.display()))]
fn format_path(
    path: &Path,
    options: PyFormatOptions,
    mode: FormatMode,
) -> Result<FormatResult, FormatterIterationError> {
    let unformatted = std::fs::read_to_string(path)
        .map_err(|err| FormatterIterationError::Read(path.to_path_buf(), err))?;
    let formatted = {
        let span = span!(Level::TRACE, "format_path_without_io", path = %path.display());
        let _enter = span.enter();
        format_module(&unformatted, options)
            .map_err(|err| FormatterIterationError::FormatModule(path.to_path_buf(), err))?
    };
    let formatted = formatted.as_code();
    if formatted.len() == unformatted.len() && formatted == unformatted {
        Ok(FormatResult::Unchanged)
    } else {
        if mode.is_write() {
            std::fs::write(path, formatted.as_bytes())
                .map_err(|err| FormatterIterationError::Write(path.to_path_buf(), err))?;
        }
        Ok(FormatResult::Formatted)
    }
}

#[derive(Debug, Clone, Copy)]
enum FormatResult {
    /// The file was formatted.
    Formatted,
    /// The file was unchanged, as the formatted contents matched the existing contents.
    Unchanged,
    /// The file was skipped, as it was not a Python file.
    Skipped,
}

#[derive(Debug, Default)]
struct FormatResultSummary {
    /// The number of files that were formatted.
    formatted: usize,
    /// The number of files that were unchanged.
    unchanged: usize,
    /// The number of files that were skipped.
    skipped: usize,
}

impl From<Vec<FormatResult>> for FormatResultSummary {
    fn from(diagnostics: Vec<FormatResult>) -> Self {
        let mut path_diagnostics = Self::default();
        for diagnostic in diagnostics {
            match diagnostic {
                FormatResult::Formatted => path_diagnostics.formatted += 1,
                FormatResult::Unchanged => path_diagnostics.unchanged += 1,
                FormatResult::Skipped => path_diagnostics.skipped += 1,
            }
        }
        path_diagnostics
    }
}

impl FormatResultSummary {
    /// Pretty-print a [`FormatResultSummary`] for user-facing display.
    fn show_user(&self, writer: &mut dyn Write, mode: FormatMode) -> Result<(), io::Error> {
        if self.formatted > 0 && self.unchanged > 0 {
            writeln!(
                writer,
                "{} file{} {}, {} file{} left unchanged",
                self.formatted,
                if self.formatted == 1 { "" } else { "s" },
                match mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check => "would be reformatted",
                },
                self.unchanged,
                if self.unchanged == 1 { "" } else { "s" },
            )
        } else if self.formatted > 0 {
            writeln!(
                writer,
                "{} file{} {}",
                self.formatted,
                if self.formatted == 1 { "" } else { "s" },
                match mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check => "would be reformatted",
                }
            )
        } else if self.unchanged > 0 {
            writeln!(
                writer,
                "{} file{} left unchanged",
                self.unchanged,
                if self.unchanged == 1 { "" } else { "s" },
            )
        } else {
            Ok(())
        }
    }
}

/// An error that can occur while formatting a set of files.
#[derive(Error, Debug)]
enum FormatterIterationError {
    #[error("Failed to traverse: {0}")]
    Ignore(#[from] ignore::Error),
    #[error("Failed to read {0}: {1}")]
    Read(PathBuf, io::Error),
    #[error("Failed to write {0}: {1}")]
    Write(PathBuf, io::Error),
    #[error("Failed to format {0}: {1}")]
    FormatModule(PathBuf, FormatModuleError),
}

impl FormatterIterationError {
    /// Pretty-print a [`FormatterIterationError`] for user-facing display.
    fn show_user(&self) {
        match self {
            Self::Ignore(err) => {
                if let ignore::Error::WithPath { path, .. } = err {
                    warn!(
                        "{}{}{} {}",
                        "Failed to format ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold(),
                        err.io_error()
                            .map_or_else(|| err.to_string(), std::string::ToString::to_string)
                    );
                } else {
                    warn!(
                        "{} {}",
                        "Encountered error:".bold(),
                        err.io_error()
                            .map_or_else(|| err.to_string(), std::string::ToString::to_string)
                    );
                }
            }
            Self::Read(path, err) => {
                warn!(
                    "{}{}{} {err}",
                    "Failed to read ".bold(),
                    fs::relativize_path(path).bold(),
                    ":".bold()
                );
            }
            Self::Write(path, err) => {
                warn!(
                    "{}{}{} {err}",
                    "Failed to write ".bold(),
                    fs::relativize_path(path).bold(),
                    ":".bold()
                );
            }
            Self::FormatModule(path, err) => {
                warn!(
                    "{}{}{} {err}",
                    "Failed to format ".bold(),
                    fs::relativize_path(path).bold(),
                    ":".bold()
                );
            }
        }
    }
}
