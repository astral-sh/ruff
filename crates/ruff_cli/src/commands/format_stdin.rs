use std::io::{stdout, Write};
use std::num::NonZeroU16;
use std::path::Path;

use anyhow::Result;
use log::warn;
use ruff::settings::types::PreviewMode;
use ruff_formatter::LineWidth;

use ruff_python_formatter::{format_module, PyFormatOptions};
use ruff_workspace::resolver::python_file_at_path;

use crate::args::{FormatArguments, Overrides};
use crate::commands::format::{FormatCommandError, FormatCommandResult, FormatMode};
use crate::resolve::resolve;
use crate::stdin::read_from_stdin;
use crate::ExitStatus;

/// Run the formatter over a single file, read from `stdin`.
pub(crate) fn format_stdin(cli: &FormatArguments, overrides: &Overrides) -> Result<ExitStatus> {
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

    let preview = match pyproject_config.settings.lib.preview {
        PreviewMode::Enabled => ruff_python_formatter::PreviewMode::Enabled,
        PreviewMode::Disabled => ruff_python_formatter::PreviewMode::Disabled,
    };
    let line_length = pyproject_config.settings.lib.line_length;

    let options = path
        .map(PyFormatOptions::from_extension)
        .unwrap_or_default()
        .with_line_width(LineWidth::from(NonZeroU16::from(line_length)))
        .with_preview(preview);

    match format_source(path, options, mode) {
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
    options: PyFormatOptions,
    mode: FormatMode,
) -> Result<FormatCommandResult, FormatCommandError> {
    let unformatted = read_from_stdin()
        .map_err(|err| FormatCommandError::Read(path.map(Path::to_path_buf), err))?;
    let formatted = format_module(&unformatted, options)
        .map_err(|err| FormatCommandError::FormatModule(path.map(Path::to_path_buf), err))?;
    let formatted = formatted.as_code();
    if formatted.len() == unformatted.len() && formatted == unformatted {
        Ok(FormatCommandResult::Unchanged)
    } else {
        if mode.is_write() {
            stdout()
                .lock()
                .write_all(formatted.as_bytes())
                .map_err(|err| FormatCommandError::Write(path.map(Path::to_path_buf), err))?;
        }
        Ok(FormatCommandResult::Formatted)
    }
}
