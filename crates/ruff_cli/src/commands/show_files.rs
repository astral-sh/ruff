use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::Result;
use itertools::Itertools;

use ruff::resolver::PyprojectDiscovery;
use ruff::{resolver, warn_user_once};

use crate::args::Overrides;

/// Show the list of files to be checked based on current settings.
pub fn show_files(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    overrides: &Overrides,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, _resolver) = resolver::python_files_in_path(files, pyproject_strategy, overrides)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(());
    }

    // Print the list of files.
    let mut stdout = BufWriter::new(io::stdout().lock());
    for entry in paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path()))
    {
        writeln!(stdout, "{}", entry.path().to_string_lossy())?;
    }

    Ok(())
}
