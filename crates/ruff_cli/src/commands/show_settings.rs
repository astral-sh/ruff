use std::io::Write;
use std::path::PathBuf;

use anyhow::{bail, Result};
use itertools::Itertools;

use ruff_workspace::resolver::{python_files_in_path, PyprojectConfig, ResolvedFile};

use crate::args::CliOverrides;

/// Print the user-facing configuration settings.
pub(crate) fn show_settings(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    overrides: &CliOverrides,
    writer: &mut impl Write,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = python_files_in_path(files, pyproject_config, overrides)?;

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

    let settings = resolver.resolve(&path);

    writeln!(writer, "Resolved settings for: {path:?}")?;
    if let Some(settings_path) = pyproject_config.path.as_ref() {
        writeln!(writer, "Settings path: {settings_path:?}")?;
    }
    write!(writer, "{settings}")?;

    Ok(())
}
