use std::io::{stdout, Write};

use anyhow::Result;

use ruff_python_formatter::{format_module, PyFormatOptions};
use ruff_workspace::resolver::python_file_at_path;

use crate::args::{FormatArguments, Overrides};
use crate::commands::format::FormatMode;
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
    let unformatted = read_from_stdin()?;
    let options = cli
        .stdin_filename
        .as_deref()
        .map(PyFormatOptions::from_extension)
        .unwrap_or_default();
    let formatted = format_module(&unformatted, options)?;

    match mode {
        FormatMode::Write => {
            stdout().lock().write_all(formatted.as_code().as_bytes())?;
            Ok(ExitStatus::Success)
        }
        FormatMode::Check => {
            if formatted.as_code().len() == unformatted.len() && formatted.as_code() == unformatted
            {
                Ok(ExitStatus::Success)
            } else {
                Ok(ExitStatus::Failure)
            }
        }
    }
}
