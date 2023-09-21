use std::io::{stdout, Write};
use std::path::Path;

use anyhow::Result;
use log::warn;

use ruff_python_ast::PySourceType;
use ruff_python_formatter::format_module;
use ruff_workspace::resolver::python_file_at_path;
use ruff_workspace::FormatterSettings;

use crate::args::{CliOverrides, FormatArguments};
use crate::commands::format::{FormatCommandError, FormatCommandResult, FormatMode};
use crate::resolve::resolve;
use crate::stdin::read_from_stdin;
use crate::ExitStatus;

/// Run the formatter over a single file, read from `stdin`.
pub(crate) fn format_stdin(cli: &FormatArguments, overrides: &CliOverrides) -> Result<ExitStatus> {
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

    if let Some(filename) = cli.stdin_filename.as_deref() {
        if !python_file_at_path(filename, &pyproject_config, overrides)? {
            return Ok(ExitStatus::Success);
        }
    }

    // Format the file.
    let path = cli.stdin_filename.as_deref();

    match format_source(path, &pyproject_config.settings.formatter, mode) {
        Ok(result) => match mode {
            FormatMode::Write => Ok(ExitStatus::Success),
            FormatMode::Check => {
                if result.is_formatted() {
                    Ok(ExitStatus::Failure)
                } else {
                    Ok(ExitStatus::Success)
                }
            }
        },
        Err(err) => {
            warn!("{err}");
            Ok(ExitStatus::Error)
        }
    }
}

/// Format source code read from `stdin`.
fn format_source(
    path: Option<&Path>,
    settings: &FormatterSettings,
    mode: FormatMode,
) -> Result<FormatCommandResult, FormatCommandError> {
    let unformatted = read_from_stdin()
        .map_err(|err| FormatCommandError::Read(path.map(Path::to_path_buf), err))?;

    let options = settings.to_format_options(
        path.map(PySourceType::from).unwrap_or_default(),
        &unformatted,
    );

    let formatted = format_module(&unformatted, options)
        .map_err(|err| FormatCommandError::FormatModule(path.map(Path::to_path_buf), err))?;
    let formatted = formatted.as_code();

    if mode.is_write() {
        stdout()
            .lock()
            .write_all(formatted.as_bytes())
            .map_err(|err| FormatCommandError::Write(path.map(Path::to_path_buf), err))?;
    }
    if formatted.len() == unformatted.len() && formatted == unformatted {
        Ok(FormatCommandResult::Unchanged)
    } else {
        Ok(FormatCommandResult::Formatted)
    }
}
