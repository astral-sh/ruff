use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use log::{debug, error};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;

use ruff_linter::linter::add_noqa_to_path;
use ruff_linter::source_kind::SourceKind;
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{
    match_exclusion, python_files_in_path, PyprojectConfig, ResolvedFile,
};

use crate::args::ConfigArguments;

/// Add `noqa` directives to a collection of files.
pub(crate) fn add_noqa(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    config_arguments: &ConfigArguments,
) -> Result<usize> {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) = python_files_in_path(files, pyproject_config, config_arguments)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(0);
    }

    // Discover the package root for each Python file.
    let package_roots = resolver.package_roots(
        &paths
            .iter()
            .flatten()
            .map(ResolvedFile::path)
            .collect::<Vec<_>>(),
    );

    let start = Instant::now();
    let modifications: usize = paths
        .par_iter()
        .flatten()
        .filter_map(|resolved_file| {
            let SourceType::Python(source_type @ (PySourceType::Python | PySourceType::Stub)) =
                SourceType::from(resolved_file.path())
            else {
                return None;
            };
            let path = resolved_file.path();
            let package = resolved_file
                .path()
                .parent()
                .and_then(|parent| package_roots.get(parent))
                .and_then(|package| *package);
            let settings = resolver.resolve(path);
            if (settings.file_resolver.force_exclude || !resolved_file.is_root())
                && match_exclusion(
                    resolved_file.path(),
                    resolved_file.file_name(),
                    &settings.linter.exclude,
                )
            {
                return None;
            }
            let source_kind = match SourceKind::from_path(path, source_type) {
                Ok(Some(source_kind)) => source_kind,
                Ok(None) => return None,
                Err(e) => {
                    error!("Failed to extract source from {}: {e}", path.display());
                    return None;
                }
            };
            match add_noqa_to_path(path, package, &source_kind, source_type, &settings.linter) {
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
