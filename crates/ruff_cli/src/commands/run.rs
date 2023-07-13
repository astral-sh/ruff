use std::collections::HashMap;
use std::fmt::Write;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use colored::Colorize;
use ignore::Error;
use itertools::Itertools;
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
use crate::cache::{self, Cache};
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

    // Load the caches.
    let caches = bool::from(cache).then(|| {
        package_roots
            .iter()
            .map(|(package, package_root)| package_root.unwrap_or(package))
            .unique()
            .par_bridge()
            .map(|cache_root| {
                let settings = resolver.resolve_all(cache_root, pyproject_config);
                let cache = Cache::open(
                    &settings.cli.cache_dir,
                    cache_root.to_path_buf(),
                    &settings.lib,
                );
                (cache_root, cache)
            })
            .collect::<HashMap<&Path, Cache>>()
    });

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

                    let settings = resolver.resolve_all(path, pyproject_config);

                    let cache_root = package.unwrap_or_else(|| path.parent().unwrap_or(path));
                    let cache = caches.as_ref().and_then(|caches| {
                        if let Some(cache) = caches.get(&cache_root) {
                            Some(cache)
                        } else {
                            debug!("No cache found for {}", cache_root.display());
                            None
                        }
                    });

                    lint_path(path, package, settings, cache, noqa, autofix).map_err(|e| {
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

    // Store the caches.
    if let Some(caches) = caches {
        caches
            .into_par_iter()
            .try_for_each(|(_, cache)| cache.store())?;
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
    cache: Option<&Cache>,
    noqa: flags::Noqa,
    autofix: flags::FixMode,
) -> Result<Diagnostics> {
    let result = catch_unwind(|| {
        crate::diagnostics::lint_path(path, package, settings, cache, noqa, autofix)
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
