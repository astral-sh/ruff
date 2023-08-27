use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use itertools::Itertools;

use ruff::warn_user_once;
use ruff_workspace::resolver::{python_files_in_path, PyprojectConfig};

use crate::args::Overrides;

/// Show the list of files to be checked based on current settings.
pub(crate) fn show_files(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    overrides: &Overrides,
    writer: &mut impl Write,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, _resolver) = python_files_in_path(files, pyproject_config, overrides)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(());
    }

    // Print the list of files.
    for entry in paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path()))
    {
        writeln!(writer, "{}", entry.path().to_string_lossy())?;
    }

    Ok(())
}
