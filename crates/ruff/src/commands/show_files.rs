use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use itertools::Itertools;

use ruff_linter::warn_user_once;
use ruff_workspace::resolver::{python_files_in_path, PyprojectConfig, ResolvedFile};

use crate::args::ConfigArguments;

/// Show the list of files to be checked based on current settings.
pub(crate) fn show_files(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    config_arguments: &ConfigArguments,
    writer: &mut impl Write,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, _resolver) = python_files_in_path(files, pyproject_config, config_arguments)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(());
    }

    // Print the list of files.
    for path in paths
        .into_iter()
        .flatten()
        .map(ResolvedFile::into_path)
        .sorted_unstable()
    {
        writeln!(writer, "{}", path.to_string_lossy())?;
    }

    Ok(())
}
