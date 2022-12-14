use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Result};
use itertools::Itertools;
use log::{debug, error};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
use rustpython_ast::Location;
use serde::Serialize;

use crate::autofix::fixer;
use crate::checks::{CheckCode, CheckKind};
use crate::cli::Overrides;
use crate::iterators::par_iter;
use crate::linter::{add_noqa_to_path, autoformat_path, lint_path, lint_stdin, Diagnostics};
use crate::message::Message;
use crate::resolver;
use crate::resolver::Strategy;
use crate::settings::types::SerializationFormat;

/// Run the linter over a collection of files.
pub fn run(
    files: &[PathBuf],
    strategy: &Strategy,
    overrides: &Overrides,
    cache: bool,
    autofix: &fixer::Mode,
) -> Result<Diagnostics> {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) = resolver::resolve_python_files(files, strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let mut diagnostics: Diagnostics = par_iter(&paths)
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    let settings = resolver.resolve(path, strategy);
                    lint_path(path, settings, &cache.into(), autofix)
                        .map_err(|e| (Some(path.to_owned()), e.to_string()))
                }
                Err(e) => Err((
                    e.path().map(Path::to_owned),
                    e.io_error()
                        .map_or_else(|| e.to_string(), io::Error::to_string),
                )),
            }
            .unwrap_or_else(|(path, message)| {
                if let Some(path) = &path {
                    let settings = resolver.resolve(path, strategy);
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
    strategy: &Strategy,
    filename: &Path,
    autofix: &fixer::Mode,
) -> Result<Diagnostics> {
    let stdin = read_from_stdin()?;
    let settings = match strategy {
        Strategy::Fixed(settings) => settings,
        Strategy::Hierarchical(settings) => settings,
    };
    let mut diagnostics = lint_stdin(filename, &stdin, settings, autofix)?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}

/// Add `noqa` directives to a collection of files.
pub fn add_noqa(files: &[PathBuf], strategy: &Strategy, overrides: &Overrides) -> Result<usize> {
    // Collect all the files to check.
    let start = Instant::now();
    let (paths, resolver) = resolver::resolve_python_files(files, strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications: usize = par_iter(&paths)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let settings = resolver.resolve(path, strategy);
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

/// Automatically format a collection of files.
pub fn autoformat(files: &[PathBuf], strategy: &Strategy, overrides: &Overrides) -> Result<usize> {
    // Collect all the files to format.
    let start = Instant::now();
    let (paths, resolver) = resolver::resolve_python_files(files, strategy, overrides)?;
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications = par_iter(&paths)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let settings = resolver.resolve(path, strategy);
            match autoformat_path(path, settings) {
                Ok(()) => Some(()),
                Err(e) => {
                    error!("Failed to autoformat {}: {e}", path.to_string_lossy());
                    None
                }
            }
        })
        .count();

    let duration = start.elapsed();
    debug!("Auto-formatted files in: {:?}", duration);

    Ok(modifications)
}

/// Print the user-facing configuration settings.
pub fn show_settings(files: &[PathBuf], strategy: &Strategy, overrides: &Overrides) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, resolver) = resolver::resolve_python_files(files, strategy, overrides)?;

    // Print the list of files.
    let Some(entry) = paths
        .iter()
        .flatten()
        .sorted_by(|a, b| a.path().cmp(b.path())).next() else {
        bail!("No files found under the given path");
    };
    let path = entry.path();
    let settings = resolver.resolve(path, strategy);
    println!("Resolved settings for: {path:?}");
    println!("{settings:#?}");

    Ok(())
}

/// Show the list of files to be checked based on current settings.
pub fn show_files(files: &[PathBuf], strategy: &Strategy, overrides: &Overrides) -> Result<()> {
    // Collect all files in the hierarchy.
    let (paths, _resolver) = resolver::resolve_python_files(files, strategy, overrides)?;

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
    };
    Ok(())
}
