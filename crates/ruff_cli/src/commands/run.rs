use std::collections::{hash_map, HashMap};
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use ignore::Error;
use log::{debug, error, warn};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
use ruff_text_size::{TextRange, TextSize};

use ruff::message::Message;
use ruff::registry::Rule;
use ruff::resolver::{PyprojectConfig, PyprojectDiscoveryStrategy};
use ruff::settings::{flags, AllSettings};
use ruff::{fs, packaging, resolver, warn_user_once, IOError};
use ruff_diagnostics::Diagnostic;
use ruff_python_ast::imports::ImportMap;
use ruff_python_ast::source_code::SourceFileBuilder;

use crate::args::Overrides;
use crate::cache::{self, PackageCache};
use crate::diagnostics::Diagnostics;
use crate::panic::catch_unwind;

/// Run the linter over a collection of files.
pub(crate) fn run(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    overrides: &Overrides,
    cache: flags::Cache,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    // Collect all the Python files to check.
    let start = Instant::now();
    let (paths, resolver) = resolver::python_files_in_path(files, pyproject_config, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(Diagnostics::default());
    }

    // Initialize the cache.
    if cache.into() {
        fn init_cache(path: &Path) {
            if let Err(e) = cache::init(path) {
                error!("Failed to initialize cache at {}: {e:?}", path.display());
            }
        }

        match pyproject_config.strategy {
            PyprojectDiscoveryStrategy::Fixed => {
                init_cache(&pyproject_config.settings.cli.cache_dir);
            }
            PyprojectDiscoveryStrategy::Hierarchical => {
                for settings in std::iter::once(&pyproject_config.settings).chain(resolver.iter()) {
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
        pyproject_config,
    );

    // Create a cache per package, if enabled.
    let package_caches = if cache.into() {
        let mut caches = HashMap::new();
        // TODO(thomas): try to merge this with the detection of package roots
        // above or with the parallel iteration below.
        for entry in &paths {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            let package = path
                .parent()
                .and_then(|parent| package_roots.get(parent))
                .and_then(|package| *package);
            // For paths not in a package, e.g. scripts, we use the path as
            // the package root.
            let package_root = package.unwrap_or(path);

            let settings = resolver.resolve_all(path, pyproject_config);

            if let hash_map::Entry::Vacant(entry) = caches.entry(package_root) {
                let cache = PackageCache::open(
                    &settings.cli.cache_dir,
                    package_root.to_owned(),
                    &settings.lib,
                );
                entry.insert(cache);
            }
        }
        Some(caches)
    } else {
        None
    };

    let start = Instant::now();
    let mut diagnostics: Diagnostics = paths
        .par_iter()
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let package = path
                        .parent()
                        .and_then(|parent| package_roots.get(parent))
                        .and_then(|package| *package);

                    let package_cache = package_caches.as_ref().map(|package_caches| {
                        let package_root = package.unwrap_or(path);
                        let package_cache = package_caches
                            .get(package_root)
                            .expect("failed to get package cache");
                        package_cache
                    });

                    let settings = resolver.resolve_all(path, pyproject_config);

                    lint_path(path, package, settings, package_cache, noqa, autofix).map_err(|e| {
                        (Some(path.to_owned()), {
                            let mut error = e.to_string();
                            for cause in e.chain() {
                                write!(&mut error, "\n  Caused by: {cause}").unwrap();
                            }
                            error
                        })
                    })
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
                    let settings = resolver.resolve(path, pyproject_config);
                    if settings.rules.enabled(Rule::IOError) {
                        let file =
                            SourceFileBuilder::new(path.to_string_lossy().as_ref(), "").finish();

                        Diagnostics::new(
                            vec![Message::from_diagnostic(
                                Diagnostic::new(IOError { message }, TextRange::default()),
                                file,
                                TextSize::default(),
                            )],
                            ImportMap::default(),
                        )
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

    diagnostics.messages.sort();

    // Store the package caches.
    if let Some(package_caches) = package_caches {
        for package_cache in package_caches.values() {
            package_cache.store()?;
        }
    }

    let duration = start.elapsed();
    debug!("Checked {:?} files in: {:?}", paths.len(), duration);

    Ok(diagnostics)
}

/// Wraps [`lint_path`](crate::diagnostics::lint_path) in a [`catch_unwind`](std::panic::catch_unwind) and emits
/// a diagnostic if the linting the file panics.
fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &AllSettings,
    package_cache: Option<&PackageCache>,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    let result = catch_unwind(|| {
        crate::diagnostics::lint_path(path, package, settings, package_cache, noqa, autofix)
    });

    match result {
        Ok(inner) => inner,
        Err(error) => {
            let message = r#"This indicates a bug in `ruff`. If you could open an issue at:

https://github.com/astral-sh/ruff/issues/new?title=%5BLinter%20panic%5D

with the relevant file contents, the `pyproject.toml` settings, and the following stack trace, we'd be very appreciative!
"#;

            warn!(
                "{}{}{} {message}\n{error}",
                "Linting panicked ".bold(),
                fs::relativize_path(path).bold(),
                ":".bold()
            );

            Ok(Diagnostics::default())
        }
    }
}
