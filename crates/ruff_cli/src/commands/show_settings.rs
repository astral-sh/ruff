use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{bail, Result};
use itertools::Itertools;

use ruff::resolver;
use ruff::resolver::PyprojectDiscovery;

use crate::args::Overrides;

/// Print the user-facing configuration settings.
pub fn show_settings(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    overrides: &Overrides,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = resolver::python_files_in_path(files, pyproject_strategy, overrides)?;

    // Print the list of files.
    let Some(entry) = paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path())).next() else {
        bail!("No files found under the given path");
    };
    let path = entry.path();
    let settings = resolver.resolve(path, pyproject_strategy);

    let mut stdout = BufWriter::new(io::stdout().lock());
    writeln!(stdout, "Resolved settings for: {path:?}")?;
    writeln!(stdout, "{settings:#?}")?;

    Ok(())
}
