use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Result, bail};
use itertools::Itertools;

use ruff_workspace::resolver::{ResolvedFile, RootProject, project_files_in_path};
use ruff_workspace::script::{ScriptSettingsError, script, should_read_script_metadata};

use crate::args::ConfigArguments;

/// Print the user-facing configuration settings.
pub(crate) fn show_settings(
    files: &[PathBuf],
    root_project: &RootProject,
    config_arguments: &ConfigArguments,
    writer: &mut impl Write,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = project_files_in_path(files, root_project, config_arguments)?;

    // Print the list of files.
    let Some(path) = paths
        .into_iter()
        .flatten()
        .map(ResolvedFile::into_path)
        .sorted_unstable()
        .next()
    else {
        bail!("No files found under the given path");
    };

    let resolved = resolver.resolve_with_path(&path);

    let source_type = resolved.settings.linter.extension.get_source_type(&path);
    let script = if should_read_script_metadata(resolved.settings.preview, source_type)
        && let Ok(source) = fs::read_to_string(&path)
    {
        script(
            resolved,
            config_arguments,
            config_arguments.script_configuration(),
            Some(&path),
            source_type,
            &source,
        )
        .map_err(ScriptSettingsError::into_anyhow)?
    } else {
        None
    };
    let (settings, config_path) = match &script {
        Some(script) => (
            script.settings(&resolved.settings),
            script.config_path(resolved),
        ),
        None => (&resolved.settings, resolved.config_path()),
    };
    writeln!(writer, "Resolved settings for: \"{}\"", path.display())?;
    if let Some(settings_path) = config_path {
        writeln!(writer, "Settings path: \"{}\"", settings_path.display())?;
    }
    write!(writer, "{settings}")?;

    Ok(())
}
