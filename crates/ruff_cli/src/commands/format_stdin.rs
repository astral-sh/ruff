use std::io::stdout;
use std::path::Path;

use anyhow::Result;
use log::warn;

use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::python_file_at_path;
use ruff_workspace::FormatterSettings;

use crate::args::{CliOverrides, FormatArguments};
use crate::commands::format::{format_source, FormatCommandError, FormatCommandResult, FormatMode};
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

    let path = cli.stdin_filename.as_deref();

    let SourceType::Python(source_type) = path.map(SourceType::from).unwrap_or_default() else {
        return Ok(ExitStatus::Success);
    };

    // Format the file.
    match format_source_code(
        path,
        &pyproject_config.settings.formatter,
        source_type,
        mode,
    ) {
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
fn format_source_code(
    path: Option<&Path>,
    settings: &FormatterSettings,
    source_type: PySourceType,
    mode: FormatMode,
) -> Result<FormatCommandResult, FormatCommandError> {
    // Read the source from stdin.
    let source_code = read_from_stdin()
        .map_err(|err| FormatCommandError::Read(path.map(Into::into), err.into()))?;
    let source_kind = SourceKind::from_source_code(source_code, source_type)
        .map_err(|err| FormatCommandError::Read(path.map(Into::into), err))?;

    // Format the source, and write to stdout regardless of the mode.
    if let Some(formatted) = format_source(&source_kind, source_type, path, settings)? {
        if mode.is_write() {
            let mut writer = stdout().lock();
            formatted
                .write(&mut writer)
                .map_err(|err| FormatCommandError::Write(path.map(Into::into), err))?;
        }

        Ok(FormatCommandResult::Formatted)
    } else {
        if mode.is_write() {
            let mut writer = stdout().lock();
            source_kind
                .write(&mut writer)
                .map_err(|err| FormatCommandError::Write(path.map(Into::into), err))?;
        }

        Ok(FormatCommandResult::Unchanged)
    }
}
