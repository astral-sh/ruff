use std::io::stdout;
use std::path::Path;

use anyhow::Result;
use log::error;

use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{match_exclusion, python_file_at_path, Resolver};
use ruff_workspace::FormatterSettings;

use crate::args::{ConfigArguments, FormatArguments, FormatRange};
use crate::commands::format::{
    format_source, warn_incompatible_formatter_settings, FormatCommandError, FormatMode,
    FormatResult, FormattedSource,
};
use crate::resolve::resolve;
use crate::stdin::{parrot_stdin, read_from_stdin};
use crate::ExitStatus;

/// Run the formatter over a single file, read from `stdin`.
pub(crate) fn format_stdin(
    cli: &FormatArguments,
    config_arguments: &ConfigArguments,
) -> Result<ExitStatus> {
    let pyproject_config = resolve(config_arguments, cli.stdin_filename.as_deref())?;

    let mut resolver = Resolver::new(&pyproject_config);
    warn_incompatible_formatter_settings(&resolver);

    let mode = FormatMode::from_cli(cli);

    if resolver.force_exclude() {
        if let Some(filename) = cli.stdin_filename.as_deref() {
            if !python_file_at_path(filename, &mut resolver, config_arguments)? {
                if mode.is_write() {
                    parrot_stdin()?;
                }
                return Ok(ExitStatus::Success);
            }

            if filename.file_name().is_some_and(|name| {
                match_exclusion(filename, name, &resolver.base_settings().formatter.exclude)
            }) {
                if mode.is_write() {
                    parrot_stdin()?;
                }
                return Ok(ExitStatus::Success);
            }
        }
    }

    let path = cli.stdin_filename.as_deref();
    let settings = &resolver.base_settings().formatter;

    let source_type = match path.and_then(|path| settings.extension.get(path)) {
        None => match path.map(SourceType::from).unwrap_or_default() {
            SourceType::Python(source_type) => source_type,
            SourceType::Toml(_) => {
                if mode.is_write() {
                    parrot_stdin()?;
                }
                return Ok(ExitStatus::Success);
            }
        },
        Some(language) => PySourceType::from(language),
    };

    // Format the file.
    match format_source_code(path, cli.range, settings, source_type, mode) {
        Ok(result) => match mode {
            FormatMode::Write => Ok(ExitStatus::Success),
            FormatMode::Check | FormatMode::Diff => {
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
    range: Option<FormatRange>,
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
    let formatted = format_source(&source_kind, source_type, path, settings, range)?;

    match &formatted {
        FormattedSource::Formatted(formatted) => match mode {
            FormatMode::Write => {
                let mut writer = stdout().lock();
                formatted
                    .write(&mut writer)
                    .map_err(|err| FormatCommandError::Write(path.map(Path::to_path_buf), err))?;
            }
            FormatMode::Check => {}
            FormatMode::Diff => {
                use std::io::Write;
                write!(
                    &mut stdout().lock(),
                    "{}",
                    source_kind.diff(formatted, path).unwrap()
                )
                .map_err(|err| FormatCommandError::Diff(path.map(Path::to_path_buf), err))?;
            }
        },
        FormattedSource::Unchanged => {
            // Write to stdout regardless of whether the source was formatted
            if mode.is_write() {
                let mut writer = stdout().lock();
                source_kind
                    .write(&mut writer)
                    .map_err(|err| FormatCommandError::Write(path.map(Path::to_path_buf), err))?;
            }
        }
    }

    Ok(FormatResult::from(formatted))
}
