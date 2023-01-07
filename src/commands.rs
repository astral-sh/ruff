use std::fs::remove_dir_all;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Result};
use colored::Colorize;
use ignore::Error;
use itertools::Itertools;
use log::{debug, error};
use path_absolutize::path_dedot;
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
use rustpython_ast::Location;
use serde::Serialize;
use walkdir::WalkDir;

use crate::autofix::fixer;
use crate::cache::DEFAULT_CACHE_DIR_NAME;
use crate::cli::Overrides;
use crate::iterators::par_iter;
use crate::linter::{add_noqa_to_path, lint_path, lint_stdin, Diagnostics};
use crate::logging::LogLevel;
use crate::message::Message;
use crate::registry::{CheckCode, CheckKind};
use crate::resolver::{FileDiscovery, PyprojectDiscovery};
use crate::settings::flags;
use crate::settings::types::SerializationFormat;
use crate::{cache, fs, one_time_warning, packages, resolver};

/// Run the linter over a collection of files.
pub fn run(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    file_strategy: &FileDiscovery,
    overrides: &Overrides,
    cache: flags::Cache,
    autofix: fixer::Mode,
) -> Result<Diagnostics> {
    // Collect all the Python files to check.
    let start = Instant::now();
    let (paths, resolver) =
        resolver::python_files_in_path(files, pyproject_strategy, file_strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        one_time_warning!(
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            "No Python files found under the given path(s)".bold()
        );
        return Ok(Diagnostics::default());
    }

    // Validate the `Settings` and return any errors.
    resolver.validate(pyproject_strategy)?;

    // Initialize the cache.
    if matches!(cache, flags::Cache::Enabled) {
        match &pyproject_strategy {
            PyprojectDiscovery::Fixed(settings) => {
                if let Err(e) = cache::init(&settings.cache_dir) {
                    error!(
                        "Failed to initialize cache at {}: {e:?}",
                        settings.cache_dir.to_string_lossy()
                    );
                }
            }
            PyprojectDiscovery::Hierarchical(default) => {
                for settings in std::iter::once(default).chain(resolver.iter()) {
                    if let Err(e) = cache::init(&settings.cache_dir) {
                        error!(
                            "Failed to initialize cache at {}: {e:?}",
                            settings.cache_dir.to_string_lossy()
                        );
                    }
                }
            }
        }
    };

    // Discover the package root for each Python file.
    let package_roots = packages::detect_package_roots(
        &paths
            .iter()
            .flatten()
            .map(ignore::DirEntry::path)
            .collect::<Vec<_>>(),
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
                    let settings = resolver.resolve(path, pyproject_strategy);
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
                    let settings = resolver.resolve(path, pyproject_strategy);
                    if settings.enabled.contains(&CheckCode::E902) {
                        Diagnostics::new(vec![Message {
                            kind: CheckKind::IOError(message),
                            location: Location::default(),
                            end_location: Location::default(),
                            fix: None,
                            filename: path.to_string_lossy().to_string(),
                            source: None,
                        }])
                    } else {
                        error!("Failed to check {}: {message}", path.to_string_lossy());
                        Diagnostics::default()
                    }
                } else {
                    error!("{message}");
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
    debug!("Checked files in: {:?}", duration);

    Ok(diagnostics)
}

/// Read a `String` from `stdin`.
fn read_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

/// Run the linter over a single file, read from `stdin`.
pub fn run_stdin(
    filename: Option<&Path>,
    pyproject_strategy: &PyprojectDiscovery,
    file_strategy: &FileDiscovery,
    overrides: &Overrides,
    autofix: fixer::Mode,
) -> Result<Diagnostics> {
    if let Some(filename) = filename {
        if !resolver::python_file_at_path(filename, pyproject_strategy, file_strategy, overrides)? {
            return Ok(Diagnostics::default());
        }
    }
    let settings = match pyproject_strategy {
        PyprojectDiscovery::Fixed(settings) => settings,
        PyprojectDiscovery::Hierarchical(settings) => settings,
    };
    let package_root = filename
        .and_then(Path::parent)
        .and_then(packages::detect_package_root);
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(filename, package_root, &stdin, settings, autofix)?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}

/// Add `noqa` directives to a collection of files.
pub fn add_noqa(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    file_strategy: &FileDiscovery,
    overrides: &Overrides,
) -> Result<usize> {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) =
        resolver::python_files_in_path(files, pyproject_strategy, file_strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    if paths.is_empty() {
        one_time_warning!(
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            "No Python files found under the given path(s)".bold()
        );
        return Ok(0);
    }

    // Validate the `Settings` and return any errors.
    resolver.validate(pyproject_strategy)?;

    let start = Instant::now();
    let modifications: usize = par_iter(&paths)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let settings = resolver.resolve(path, pyproject_strategy);
            match add_noqa_to_path(path, settings) {
                Ok(count) => Some(count),
                Err(e) => {
                    error!("Failed to add noqa to {}: {e}", path.to_string_lossy());
                    None
                }
            }
        })
        .sum();

    let duration = start.elapsed();
    debug!("Added noqa to files in: {:?}", duration);

    Ok(modifications)
}

/// Print the user-facing configuration settings.
pub fn show_settings(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    file_strategy: &FileDiscovery,
    overrides: &Overrides,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) =
        resolver::python_files_in_path(files, pyproject_strategy, file_strategy, overrides)?;

    // Validate the `Settings` and return any errors.
    resolver.validate(pyproject_strategy)?;

    // Print the list of files.
    let Some(entry) = paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path())).next() else {
        bail!("No files found under the given path");
    };
    let path = entry.path();
    let settings = resolver.resolve(path, pyproject_strategy);
    println!("Resolved settings for: {path:?}");
    println!("{settings:#?}");

    Ok(())
}

/// Show the list of files to be checked based on current settings.
pub fn show_files(
    files: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    file_strategy: &FileDiscovery,
    overrides: &Overrides,
) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) =
        resolver::python_files_in_path(files, pyproject_strategy, file_strategy, overrides)?;

    if paths.is_empty() {
        one_time_warning!(
            "{}{} {}",
            "warning".yellow().bold(),
            ":".bold(),
            "No Python files found under the given path(s)".bold()
        );
        return Ok(());
    }

    // Validate the `Settings` and return any errors.
    resolver.validate(pyproject_strategy)?;

    // Print the list of files.
    for entry in paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path()))
    {
        println!("{}", entry.path().to_string_lossy());
    }

    Ok(())
}

#[derive(Serialize)]
struct Explanation<'a> {
    code: &'a str,
    category: &'a str,
    summary: &'a str,
}

/// Explain a `CheckCode` to the user.
pub fn explain(code: &CheckCode, format: &SerializationFormat) -> Result<()> {
    match format {
        SerializationFormat::Text | SerializationFormat::Grouped => {
            println!(
                "{} ({}): {}",
                code.as_ref(),
                code.category().title(),
                code.kind().summary()
            );
        }
        SerializationFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&Explanation {
                    code: code.as_ref(),
                    category: code.category().title(),
                    summary: &code.kind().summary(),
                })?
            );
        }
        SerializationFormat::Junit => {
            bail!("`--explain` does not support junit format")
        }
        SerializationFormat::Github => {
            bail!("`--explain` does not support GitHub format")
        }
        SerializationFormat::Gitlab => {
            bail!("`--explain` does not support GitLab format")
        }
    };
    Ok(())
}

/// Clear any caches in the current directory or any subdirectories.
pub fn clean(level: &LogLevel) -> Result<()> {
    for entry in WalkDir::new(&*path_dedot::CWD)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_dir())
    {
        let cache = entry.path().join(DEFAULT_CACHE_DIR_NAME);
        if cache.is_dir() {
            if level >= &LogLevel::Default {
                eprintln!("Removing cache at: {}", fs::relativize_path(&cache).bold());
            }
            remove_dir_all(&cache)?;
        }
    }
    Ok(())
}
