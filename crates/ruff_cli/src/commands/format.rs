use std::io;
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use log::warn;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use tracing::{span, Level};

use ruff::fs;
use ruff::warn_user_once;
use ruff_formatter::LineWidth;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{format_module, FormatModuleError, PyFormatOptions};
use ruff_workspace::resolver::python_files_in_path;

use crate::args::{Arguments, Overrides};
use crate::resolve::resolve;
use crate::ExitStatus;

/// Format a set of files, and return the exit status.
pub(crate) fn format(cli: &Arguments, overrides: &Overrides) -> Result<ExitStatus> {
    let pyproject_config = resolve(
        cli.isolated,
        cli.config.as_deref(),
        overrides,
        cli.stdin_filename.as_deref(),
    )?;
    let (paths, resolver) = python_files_in_path(&cli.files, &pyproject_config, overrides)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(ExitStatus::Success);
    }

    let result = paths
        .into_par_iter()
        .map(|dir_entry| {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();
            let source_type = PySourceType::from(path);
            if !(source_type.is_python() || source_type.is_stub())
                || path
                    .extension()
                    .is_some_and(|extension| extension == "toml")
            {
                return Ok(());
            }

            let line_length = resolver.resolve(path, &pyproject_config).line_length;
            let options = PyFormatOptions::from_extension(path)
                .with_line_width(LineWidth::from(NonZeroU16::from(line_length)));

            format_path(path, options)
        })
        .map(|result| {
            result.map_err(|err| {
                err.show_user();
                err
            })
        })
        .collect::<Result<Vec<_>, _>>();

    if result.is_ok() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Error)
    }
}

#[tracing::instrument(skip_all, fields(path = %path.display()))]
fn format_path(path: &Path, options: PyFormatOptions) -> Result<(), FormatterIterationError> {
    let unformatted = std::fs::read_to_string(path)
        .map_err(|err| FormatterIterationError::Read(path.to_path_buf(), err))?;
    let formatted = {
        let span = span!(Level::TRACE, "format_path_without_io", path = %path.display());
        let _enter = span.enter();
        format_module(&unformatted, options)
            .map_err(|err| FormatterIterationError::FormatModule(path.to_path_buf(), err))?
    };
    std::fs::write(path, formatted.as_code().as_bytes())
        .map_err(|err| FormatterIterationError::Write(path.to_path_buf(), err))?;
    Ok(())
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
