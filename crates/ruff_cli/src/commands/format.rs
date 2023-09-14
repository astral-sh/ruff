use std::fmt::{Display, Formatter};
use std::io;
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use log::error;
use rayon::iter::Either::{Left, Right};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use tracing::{debug, warn};

use ruff::fs;
use ruff::logging::LogLevel;
use ruff::settings::types::PreviewMode;
use ruff::warn_user_once;
use ruff_formatter::LineWidth;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_python_formatter::{format_module, FormatModuleError, PyFormatOptions};
use ruff_source_file::{find_newline, LineEnding};
use ruff_workspace::resolver::python_files_in_path;

use crate::args::{FormatArguments, Overrides};
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
        .filter_map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();

                    let SourceType::Python(
                        source_type @ (PySourceType::Python | PySourceType::Stub),
                    ) = SourceType::from(path)
                    else {
                        // Ignore any non-Python files.
                        return None;
                    };

                    let resolved_settings = resolver.resolve(path, &pyproject_config);

                    let preview = match resolved_settings.preview {
                        PreviewMode::Enabled => ruff_python_formatter::PreviewMode::Enabled,
                        PreviewMode::Disabled => ruff_python_formatter::PreviewMode::Disabled,
                    };
                    let line_length = resolved_settings.line_length;

                    let options = PyFormatOptions::from_source_type(source_type)
                        .with_line_width(LineWidth::from(NonZeroU16::from(line_length)))
                        .with_preview(preview);
                    debug!("Formatting {} with {:?}", path.display(), options);

                    Some(match catch_unwind(|| format_path(path, options, mode)) {
                        Ok(inner) => inner,
                        Err(error) => {
                            Err(FormatCommandError::Panic(Some(path.to_path_buf()), error))
                        }
                    })
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

    let summary = FormatResultSummary::new(results, mode);

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

/// Format the file at the given [`Path`].
#[tracing::instrument(skip_all, fields(path = %path.display()))]
fn format_path(
    path: &Path,
    options: PyFormatOptions,
    mode: FormatMode,
) -> Result<FormatCommandResult, FormatCommandError> {
    let unformatted = std::fs::read_to_string(path)
        .map_err(|err| FormatCommandError::Read(Some(path.to_path_buf()), err))?;

    let line_ending = match find_newline(&unformatted) {
        Some((_, LineEnding::Lf)) | None => ruff_formatter::printer::LineEnding::LineFeed,
        Some((_, LineEnding::Cr)) => ruff_formatter::printer::LineEnding::CarriageReturn,
        Some((_, LineEnding::CrLf)) => ruff_formatter::printer::LineEnding::CarriageReturnLineFeed,
    };

    let options = options.with_line_ending(line_ending);

    let formatted = format_module(&unformatted, options)
        .map_err(|err| FormatCommandError::FormatModule(Some(path.to_path_buf()), err))?;

    let formatted = formatted.as_code();
    if formatted.len() == unformatted.len() && formatted == unformatted {
        Ok(FormatCommandResult::Unchanged)
    } else {
        if mode.is_write() {
            std::fs::write(path, formatted.as_bytes())
                .map_err(|err| FormatCommandError::Write(Some(path.to_path_buf()), err))?;
        }
        Ok(FormatCommandResult::Formatted)
    }
}

#[derive(Debug, Clone, Copy, is_macro::Is)]
pub(crate) enum FormatCommandResult {
    /// The file was formatted.
    Formatted,
    /// The file was unchanged, as the formatted contents matched the existing contents.
    Unchanged,
}

#[derive(Debug)]
struct FormatResultSummary {
    /// The format mode that was used.
    mode: FormatMode,
    /// The number of files that were formatted.
    formatted: usize,
    /// The number of files that were unchanged.
    unchanged: usize,
}

impl FormatResultSummary {
    fn new(diagnostics: Vec<FormatCommandResult>, mode: FormatMode) -> Self {
        let mut summary = Self {
            mode,
            formatted: 0,
            unchanged: 0,
        };
        for diagnostic in diagnostics {
            match diagnostic {
                FormatCommandResult::Formatted => summary.formatted += 1,
                FormatCommandResult::Unchanged => summary.unchanged += 1,
            }
        }
        summary
    }
}

impl Display for FormatResultSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.formatted > 0 && self.unchanged > 0 {
            write!(
                f,
                "{} file{} {}, {} file{} left unchanged",
                self.formatted,
                if self.formatted == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check => "would be reformatted",
                },
                self.unchanged,
                if self.unchanged == 1 { "" } else { "s" },
            )
        } else if self.formatted > 0 {
            write!(
                f,
                "{} file{} {}",
                self.formatted,
                if self.formatted == 1 { "" } else { "s" },
                match self.mode {
                    FormatMode::Write => "reformatted",
                    FormatMode::Check => "would be reformatted",
                }
            )
        } else if self.unchanged > 0 {
            write!(
                f,
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
pub(crate) enum FormatCommandError {
    Ignore(#[from] ignore::Error),
    Read(Option<PathBuf>, io::Error),
    Write(Option<PathBuf>, io::Error),
    FormatModule(Option<PathBuf>, FormatModuleError),
    Panic(Option<PathBuf>, PanicError),
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
            Self::FormatModule(path, err) => {
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
