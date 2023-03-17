use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use log::{debug, error};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;

use ruff::linter::add_noqa_to_path;
use ruff::resolver::PyprojectDiscovery;
use ruff::{packaging, resolver, warn_user_once};

use crate::args::Overrides;

/// Add `noqa` directives to a collection of files.
pub fn add_noqa(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    overrides: &Overrides,
) -> Result<usize> {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) = resolver::python_files_in_path(files, pyproject_strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(0);
    }

    // Discover the package root for each Python file.
    let package_roots = packaging::detect_package_roots(
        &paths
            .iter()
            .flatten()
            .map(ignore::DirEntry::path)
            .collect::<Vec<_>>(),
        &resolver,
        pyproject_strategy,
    );

    let start = Instant::now();
    let modifications: usize = paths
        .par_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let package = path
                .parent()
                .and_then(|parent| package_roots.get(parent))
                .and_then(|package| *package);
            let settings = resolver.resolve(path, pyproject_strategy);
            match add_noqa_to_path(path, package, settings) {
                Ok(count) => Some(count),
                Err(e) => {
                    error!("Failed to add noqa to {}: {e}", path.display());
                    None
                }
            }
        })
        .sum();

    let duration = start.elapsed();
    debug!("Added noqa to files in: {:?}", duration);

    Ok(modifications)
}
