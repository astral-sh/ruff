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
use ruff_linter::settings::types::UnsafeFixes;
use rustc_hash::FxHashMap;

use ruff_diagnostics::Diagnostic;
use ruff_linter::message::Message;
use ruff_linter::registry::Rule;
use ruff_linter::settings::{flags, LinterSettings};
use ruff_linter::{fs, warn_user_once, IOError};
use ruff_python_ast::imports::ImportMap;
use ruff_source_file::SourceFileBuilder;
use ruff_text_size::{TextRange, TextSize};
use ruff_workspace::resolver::{
    match_exclusion, python_files_in_path, PyprojectConfig, PyprojectDiscoveryStrategy,
    ResolvedFile,
};

use crate::args::CliOverrides;
use crate::cache::{self, Cache};
use crate::diagnostics::Diagnostics;
use crate::panic::catch_unwind;

/// Run the linter over a collection of files.
pub(crate) fn check(
    files: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    overrides: &CliOverrides,
    cache: flags::Cache,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
    unsafe_fixes: UnsafeFixes,
) -> Result<Diagnostics> {
    // Collect all the Python files to check.
    let start = Instant::now();
    let (paths, resolver) = python_files_in_path(files, pyproject_config, overrides)?;
    debug!("Identified files to lint in: {:?}", start.elapsed());

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
                init_cache(&pyproject_config.settings.cache_dir);
            }
            PyprojectDiscoveryStrategy::Hierarchical => {
                for settings in
                    std::iter::once(&pyproject_config.settings).chain(resolver.settings())
                {
                    init_cache(&settings.cache_dir);
                }
            }
        }
    };

    // Discover the package root for each Python file.
    let package_roots = resolver.package_roots(
        &paths
            .iter()
            .flatten()
            .map(ResolvedFile::path)
            .collect::<Vec<_>>(),
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
                let settings = resolver.resolve(cache_root, pyproject_config);
                let cache = Cache::open(cache_root.to_path_buf(), settings);
                (cache_root, cache)
            })
            .collect::<HashMap<&Path, Cache>>()
    });

    let start = Instant::now();
    let diagnostics_per_file = paths.par_iter().filter_map(|resolved_file| {
        let result = match resolved_file {
            Ok(resolved_file) => {
                let path = resolved_file.path();
                let package = path
                    .parent()
                    .and_then(|parent| package_roots.get(parent))
                    .and_then(|package| *package);

                let settings = resolver.resolve(path, pyproject_config);

                if !resolved_file.is_root()
                    && match_exclusion(
                        resolved_file.path(),
                        resolved_file.file_name(),
                        &settings.linter.exclude,
                    )
                {
                    return None;
                }

                let cache_root = package.unwrap_or_else(|| path.parent().unwrap_or(path));
                let cache = caches.as_ref().and_then(|caches| {
                    if let Some(cache) = caches.get(&cache_root) {
                        Some(cache)
                    } else {
                        debug!("No cache found for {}", cache_root.display());
                        None
                    }
                });

                lint_path(
                    path,
                    package,
                    &settings.linter,
                    cache,
                    noqa,
                    fix_mode,
                    unsafe_fixes,
                )
                .map_err(|e| {
                    (Some(path.to_path_buf()), {
                        let mut error = e.to_string();
                        for cause in e.chain() {
                            write!(&mut error, "\n  Cause: {cause}").unwrap();
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
        };

        Some(result.unwrap_or_else(|(path, message)| {
            if let Some(path) = &path {
                let settings = resolver.resolve(path, pyproject_config);
                if settings.linter.rules.enabled(Rule::IOError) {
                    let dummy =
                        SourceFileBuilder::new(path.to_string_lossy().as_ref(), "").finish();

                    Diagnostics::new(
                        vec![Message::from_diagnostic(
                            Diagnostic::new(IOError { message }, TextRange::default()),
                            dummy,
                            TextSize::default(),
                        )],
                        ImportMap::default(),
                        FxHashMap::default(),
                    )
                } else {
                    warn!(
                        "{}{}{} {message}",
                        "Failed to lint ".bold(),
                        fs::relativize_path(path).bold(),
                        ":".bold()
                    );
                    Diagnostics::default()
                }
            } else {
                warn!("{} {message}", "Encountered error:".bold());
                Diagnostics::default()
            }
        }))
    });

    // Aggregate the diagnostics of all checked files and count the checked files.
    // This can't be a regular for loop because we use `par_iter`.
    let (mut all_diagnostics, checked_files) = diagnostics_per_file
        .fold(
            || (Diagnostics::default(), 0u64),
            |(all_diagnostics, checked_files), file_diagnostics| {
                (all_diagnostics + file_diagnostics, checked_files + 1)
            },
        )
        .reduce(
            || (Diagnostics::default(), 0u64),
            |a, b| (a.0 + b.0, a.1 + b.1),
        );

    all_diagnostics.messages.sort();

    // Store the caches.
    if let Some(caches) = caches {
        caches
            .into_par_iter()
            .try_for_each(|(_, cache)| cache.store())?;
    }

    let duration = start.elapsed();
    debug!("Checked {:?} files in: {:?}", checked_files, duration);

    Ok(all_diagnostics)
}

/// Wraps [`lint_path`](crate::diagnostics::lint_path) in a [`catch_unwind`](std::panic::catch_unwind) and emits
/// a diagnostic if the linting the file panics.
fn lint_path(
    path: &Path,
    package: Option<&Path>,
    settings: &LinterSettings,
    cache: Option<&Cache>,
    noqa: flags::Noqa,
    fix_mode: flags::FixMode,
    unsafe_fixes: UnsafeFixes,
) -> Result<Diagnostics> {
    let result = catch_unwind(|| {
        crate::diagnostics::lint_path(path, package, settings, cache, noqa, fix_mode, unsafe_fixes)
    });

    match result {
        Ok(inner) => inner,
        Err(error) => {
            let message = r#"This indicates a bug in Ruff. If you could open an issue at:

    https://github.com/astral-sh/ruff/issues/new?title=%5BLinter%20panic%5D

...with the relevant file contents, the `pyproject.toml` settings, and the following stack trace, we'd be very appreciative!
"#;

            error!(
                "{}{}{} {message}\n{error}",
                "Panicked while linting ".bold(),
                fs::relativize_path(path).bold(),
                ":".bold()
            );

            Ok(Diagnostics::default())
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod test {
    use std::fs;
    use std::os::unix::fs::OpenOptionsExt;

    use anyhow::Result;

    use ruff_linter::settings::types::UnsafeFixes;
    use rustc_hash::FxHashMap;
    use tempfile::TempDir;

    use ruff_linter::message::{Emitter, EmitterContext, TextEmitter};
    use ruff_linter::registry::Rule;
    use ruff_linter::settings::{flags, LinterSettings};
    use ruff_workspace::resolver::{PyprojectConfig, PyprojectDiscoveryStrategy};
    use ruff_workspace::Settings;

    use crate::args::CliOverrides;

    use super::check;

    /// We check that regular python files, pyproject.toml and jupyter notebooks all handle io
    /// errors gracefully
    #[test]
    fn unreadable_files() -> Result<()> {
        let path = "E902.py";
        let rule_code = Rule::IOError;

        // Create inaccessible files
        let tempdir = TempDir::new()?;
        let pyproject_toml = tempdir.path().join("pyproject.toml");
        let python_file = tempdir.path().join("code.py");
        let notebook = tempdir.path().join("notebook.ipynb");
        for file in [&pyproject_toml, &python_file, &notebook] {
            fs::OpenOptions::new()
                .create(true)
                .write(true)
                .mode(0o000)
                .open(file)?;
        }

        // Configure
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path);
        // invalid pyproject.toml is not active by default
        let settings = Settings {
            linter: LinterSettings::for_rules(vec![rule_code, Rule::InvalidPyprojectToml]),
            ..Settings::default()
        };
        let pyproject_config =
            PyprojectConfig::new(PyprojectDiscoveryStrategy::Fixed, settings, None);

        // Run
        let diagnostics = check(
            // Notebooks are not included by default
            &[tempdir.path().to_path_buf(), notebook],
            &pyproject_config,
            &CliOverrides::default(),
            flags::Cache::Disabled,
            flags::Noqa::Disabled,
            flags::FixMode::Generate,
            UnsafeFixes::Enabled,
        )
        .unwrap();
        let mut output = Vec::new();

        TextEmitter::default()
            .with_show_fix_status(true)
            .emit(
                &mut output,
                &diagnostics.messages,
                &EmitterContext::new(&FxHashMap::default()),
            )
            .unwrap();

        let messages = String::from_utf8(output).unwrap();

        insta::with_settings!({
            omit_expression => true,
            filters => vec![
                // The tempdir is always different (and platform dependent)
                (tempdir.path().to_str().unwrap(), "/home/ferris/project"),
            ]
        }, {
            insta::assert_snapshot!(snapshot, messages);
        });
        Ok(())
    }
}
