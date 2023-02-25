use std::io::{self};
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use ignore::Error;
use log::{debug, error};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;

use ruff::message::{Location, Message};
use ruff::registry::{Diagnostic, Rule};
use ruff::resolver::PyprojectDiscovery;
use ruff::settings::flags;
use ruff::{fix, fs, packaging, resolver, warn_user_once, IOError, Range};

use crate::args::Overrides;
use crate::cache;
use crate::diagnostics::{lint_path, Diagnostics};
use crate::iterators::par_iter;

/// Run the linter over a collection of files.
pub fn run(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    overrides: &Overrides,
    cache: flags::Cache,
    autofix: fix::FixMode,
) -> Result<Diagnostics> {
    // Collect all the Python files to check.
    let start = Instant::now();
    let (paths, resolver) = resolver::python_files_in_path(files, pyproject_strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(Diagnostics::default());
    }

    // Initialize the cache.
    if cache.into() {
        fn init_cache(path: &std::path::Path) {
            if let Err(e) = cache::init(path) {
                error!(
                    "Failed to initialize cache at {}: {e:?}",
                    path.to_string_lossy()
                );
            }
        }

        match &pyproject_strategy {
            PyprojectDiscovery::Fixed(settings) => {
                init_cache(&settings.cli.cache_dir);
            }
            PyprojectDiscovery::Hierarchical(default) => {
                for settings in std::iter::once(default).chain(resolver.iter()) {
                    init_cache(&settings.cli.cache_dir);
                }
            }
        }
    };

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
    let mut diagnostics: Diagnostics = par_iter(&paths)
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let package = path
                        .parent()
                        .and_then(|parent| package_roots.get(parent))
                        .and_then(|package| *package);
                    let settings = resolver.resolve_all(path, pyproject_strategy);
                    lint_path(path, package, settings, cache, autofix)
                        .map_err(|e| (Some(path.to_owned()), e.to_string()))
                }
                Err(e) => Err((
                    if let Error::WithPath { path, .. } = e {
                        Some(path.clone())
                    } else {
                        None
                    },
                    e.io_error()
                        .map_or_else(|| e.to_string(), io::Error::to_string),
                )),
            }
            .unwrap_or_else(|(path, message)| {
                if let Some(path) = &path {
                    error!(
                        "{}{}{} {message}",
                        "Failed to lint ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    );
                    let settings = resolver.resolve(path, pyproject_strategy);
                    if settings.rules.enabled(&Rule::IOError) {
                        Diagnostics::new(vec![Message::from_diagnostic(
                            Diagnostic::new(
                                IOError { message },
                                Range::new(Location::default(), Location::default()),
                            ),
                            format!("{}", path.display()),
                            None,
                            1,
                        )])
                    } else {
                        Diagnostics::default()
                    }
                } else {
                    error!("{} {message}", "Encountered error:".bold());
                    Diagnostics::default()
                }
            })
        })
        .reduce(Diagnostics::default, |mut acc, item| {
            acc += item;
            acc
        });

    diagnostics.messages.sort_unstable();
    let duration = start.elapsed();
    debug!("Checked {:?} files in: {:?}", paths.len(), duration);

    Ok(diagnostics)
}
