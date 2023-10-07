use std::io::stdout;
use std::path::Path;

use anyhow::Result;
use log::error;

use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::python_file_at_path;
use ruff_workspace::FormatterSettings;

use crate::args::{CliOverrides, FormatArguments};
use crate::commands::format::{format_source, FormatCommandError, FormatMode, FormatResult};
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
            error!("{err}");
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
) -> Result<FormatResult, FormatCommandError> {
    // Read the source from stdin.
    let source_code = read_from_stdin()
        .map_err(|err| FormatCommandError::Read(path.map(Path::to_path_buf), err.into()))?;

    let source_kind = match SourceKind::from_source_code(source_code, source_type) {
        Ok(Some(source_kind)) => source_kind,
        Ok(None) => return Ok(FormatResult::Unchanged),
        Err(err) => {
            return Err(FormatCommandError::Read(path.map(Path::to_path_buf), err));
        }
    };

    // Format the source.
    let formatted = format_source(source_kind, source_type, path, settings)?;

    // Write to stdout regardless of whether the source was formatted.
    if mode.is_write() {
        let mut writer = stdout().lock();
        formatted
            .source_kind()
            .write(&mut writer)
            .map_err(|err| FormatCommandError::Write(path.map(Path::to_path_buf), err))?;
    }

    Ok(FormatResult::from(formatted))
}
