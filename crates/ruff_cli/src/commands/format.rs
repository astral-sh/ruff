use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;
use colored::Colorize;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use thiserror::Error;
use tracing::{span, Level};

use ruff::resolver::python_files_in_path;
use ruff::warn_user_once;
use ruff_formatter::LineWidth;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{format_module, FormatModuleError, PyFormatOptions};

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
            // TODO(konstin): Unify `LineWidth` and `LineLength`
            let line_width = LineWidth::try_from(
                u16::try_from(line_length.get()).expect("Line shouldn't be larger than 2**16"),
            )
            .expect("Configured line length is too large for the formatter");
            let options = PyFormatOptions::from_extension(path).with_line_width(line_width);

            format_path(path, options)
        })
        .map(|result| {
            if let Err(err) = result.as_ref() {
                // The inner errors are all flat, i.e., none of them has a source.
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("{}", err.to_string().red().bold());
                }
            }
            result
        })
        .collect::<Result<Vec<_>, _>>();

    if result.is_ok() {
        Ok(ExitStatus::Success)
    } else {
        Ok(ExitStatus::Error)
    }
}

/// An error that can occur while formatting a set of files.
#[derive(Error, Debug)]
enum FormatterIterationError {
    #[error("Failed to traverse the inputs paths: {0}")]
    Ignore(#[from] ignore::Error),
    #[error("Failed to read {0}: {1}")]
    Read(PathBuf, io::Error),
    #[error("Failed to write {0}: {1}")]
    Write(PathBuf, io::Error),
    #[error("Failed to format {0}: {1}")]
    FormatModule(PathBuf, FormatModuleError),
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
