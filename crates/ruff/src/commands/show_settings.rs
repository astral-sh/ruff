use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Result, bail};
use itertools::Itertools;

use ruff_workspace::resolver::{PyprojectDiscovery, ResolvedFile, project_files_in_path};

use crate::args::ConfigArguments;

/// Print the user-facing configuration settings.
pub(crate) fn show_settings(
    files: &[PathBuf],
    project_config: &PyprojectDiscovery,
    config_arguments: &ConfigArguments,
    writer: &mut impl Write,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = project_files_in_path(files, project_config, config_arguments)?;

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
    let script = if resolved.settings.should_read_script_metadata(source_type)
        && let Ok(source) = fs::read_to_string(&path)
    {
        resolved.with_overrides(config_arguments).resolve_script(
            Some(&path),
            source_type,
            &source,
        )?
    } else {
        None
    };
    let resolved = script.as_ref().unwrap_or(resolved);
    writeln!(writer, "Resolved settings for: \"{}\"", path.display())?;
    if let Some(settings_path) = &resolved.config_path {
        writeln!(writer, "Settings path: \"{}\"", settings_path.display())?;
    }
    write!(writer, "{}", resolved.settings)?;

    Ok(())
}
