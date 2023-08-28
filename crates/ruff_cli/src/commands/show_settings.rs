use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Result};
use itertools::Itertools;

use ruff_workspace::resolver::{python_files_in_path, PyprojectConfig};

use crate::args::Overrides;

/// Print the user-facing configuration settings.
pub(crate) fn show_settings(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    overrides: &Overrides,
    writer: &mut impl Write,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = python_files_in_path(files, pyproject_config, overrides)?;

    // Print the list of files.
    let Some(entry) = paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path()))
        .next()
    else {
        bail!("No files found under the given path");
    };
    let path = entry.path();
    let settings = resolver.resolve(path, pyproject_config);

    writeln!(writer, "Resolved settings for: {path:?}")?;
    if let Some(settings_path) = pyproject_config.path.as_ref() {
        writeln!(writer, "Settings path: {settings_path:?}")?;
    }
    writeln!(writer, "{settings:#?}")?;

    Ok(())
}
