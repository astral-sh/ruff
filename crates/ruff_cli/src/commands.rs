use std::fs::remove_dir_all;
use std::io::{self, BufWriter, Read, Write};
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
use ruff::cache::CACHE_DIR_NAME;
use ruff::linter::add_noqa_to_path;
use ruff::logging::LogLevel;
use ruff::message::{Location, Message};
use ruff::registry::{Linter, Rule, RuleNamespace};
use ruff::resolver::PyprojectDiscovery;
use ruff::settings::flags;
use ruff::{fix, fs, packaging, resolver, warn_user_once, AutofixAvailability, IOError};
use serde::Serialize;
use walkdir::WalkDir;

use crate::args::{HelpFormat, Overrides};
use crate::cache;
use crate::diagnostics::{lint_path, lint_stdin, Diagnostics};
use crate::iterators::par_iter;

pub mod linter;

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
    if matches!(cache, flags::Cache::Enabled) {
        match &pyproject_strategy {
            PyprojectDiscovery::Fixed(settings) => {
                if let Err(e) = cache::init(&settings.cli.cache_dir) {
                    error!(
                        "Failed to initialize cache at {}: {e:?}",
                        settings.cli.cache_dir.to_string_lossy()
                    );
                }
            }
            PyprojectDiscovery::Hierarchical(default) => {
                for settings in std::iter::once(default).chain(resolver.iter()) {
                    if let Err(e) = cache::init(&settings.cli.cache_dir) {
                        error!(
                            "Failed to initialize cache at {}: {e:?}",
                            settings.cli.cache_dir.to_string_lossy()
                        );
                    }
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
                    let settings = resolver.resolve(path, pyproject_strategy);
                    if settings.rules.enabled(&Rule::IOError) {
                        Diagnostics::new(vec![Message {
                            kind: IOError { message }.into(),
                            location: Location::default(),
                            end_location: Location::default(),
                            fix: None,
                            filename: format!("{}", path.display()),
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
    debug!("Checked {:?} files in: {:?}", paths.len(), duration);

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
    overrides: &Overrides,
    autofix: fix::FixMode,
) -> Result<Diagnostics> {
    if let Some(filename) = filename {
        if !resolver::python_file_at_path(filename, pyproject_strategy, overrides)? {
            return Ok(Diagnostics::default());
        }
    }
    let settings = match pyproject_strategy {
        PyprojectDiscovery::Fixed(settings) => settings,
        PyprojectDiscovery::Hierarchical(settings) => settings,
    };
    let package_root = filename
        .and_then(Path::parent)
        .and_then(|path| packaging::detect_package_root(path, &settings.lib.namespace_packages));
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(filename, package_root, &stdin, &settings.lib, autofix)?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}

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
    write!(stdout, "Resolved settings for: {path:?}")?;
    write!(stdout, "{settings:#?}")?;

    Ok(())
}

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

#[derive(Serialize)]
struct Explanation<'a> {
    code: &'a str,
    linter: &'a str,
    summary: &'a str,
}

/// Explain a `Rule` to the user.
pub fn rule(rule: &Rule, format: HelpFormat) -> Result<()> {
    let (linter, _) = Linter::parse_code(rule.code()).unwrap();
    let mut stdout = BufWriter::new(io::stdout().lock());
    match format {
        HelpFormat::Text => {
            let mut output = String::new();
            output.push_str(&format!("# {} ({})", rule.as_ref(), rule.code()));
            output.push('\n');
            output.push('\n');

            let (linter, _) = Linter::parse_code(rule.code()).unwrap();
            output.push_str(&format!("Derived from the **{}** linter.", linter.name()));
            output.push('\n');
            output.push('\n');

            if let Some(autofix) = rule.autofixable() {
                output.push_str(match autofix.available {
                    AutofixAvailability::Sometimes => "Autofix is sometimes available.",
                    AutofixAvailability::Always => "Autofix is always available.",
                });
                output.push('\n');
                output.push('\n');
            }

            if let Some(explanation) = rule.explanation() {
                output.push_str(explanation.trim());
            } else {
                output.push_str("Message formats:");
                for format in rule.message_formats() {
                    output.push('\n');
                    output.push_str(&format!("* {}", format));
                }
            }

            writeln!(stdout, "{}", output)?;
        }
        HelpFormat::Json => {
            writeln!(
                stdout,
                "{}",
                serde_json::to_string_pretty(&Explanation {
                    code: rule.code(),
                    linter: linter.name(),
                    summary: rule.message_formats()[0],
                })?
            )?;
        }
    };
    Ok(())
}

/// Clear any caches in the current directory or any subdirectories.
pub fn clean(level: LogLevel) -> Result<()> {
    let mut stderr = BufWriter::new(io::stderr().lock());
    for entry in WalkDir::new(&*path_dedot::CWD)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
    {
        let cache = entry.path().join(CACHE_DIR_NAME);
        if cache.is_dir() {
            if level >= LogLevel::Default {
                writeln!(
                    stderr,
                    "Removing cache at: {}",
                    fs::relativize_path(&cache).bold()
                )?;
            }
            remove_dir_all(&cache)?;
        }
    }
    Ok(())
}
