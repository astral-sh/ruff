use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{bail, Result};
use itertools::Itertools;

use ruff::resolver;
use ruff::resolver::PyprojectConfig;

use crate::args::Overrides;

/// Print the user-facing configuration settings.
pub fn show_settings(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    overrides: &Overrides,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = resolver::python_files_in_path(files, pyproject_config, overrides)?;

    // Print the list of files.
    let Some(entry) = paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path())).next() else {
        bail!("No files found under the given path");
    };
    let path = entry.path();
    let settings = resolver.resolve(path, pyproject_config);

    let mut stdout = BufWriter::new(io::stdout().lock());
    writeln!(stdout, "Resolved settings for: {path:?}")?;
    if let Some(settings_path) = pyproject_config.path.as_ref() {
        writeln!(stdout, "Settings path: {settings_path:?}")?;
    }
    writeln!(stdout, "{settings:#?}")?;

    Ok(())
}
