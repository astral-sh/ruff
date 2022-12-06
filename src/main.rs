#![allow(
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::implicit_hasher,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::Instant;

use ::ruff::autofix::fixer;
use ::ruff::checks::{CheckCode, CheckKind};
use ::ruff::cli::{collect_per_file_ignores, extract_log_level, Cli};
use ::ruff::fs::iter_python_files;
use ::ruff::linter::{add_noqa_to_path, autoformat_path, lint_path, lint_stdin, Diagnostics};
use ::ruff::logging::{set_up_logging, LogLevel};
use ::ruff::message::Message;
use ::ruff::printer::Printer;
use ::ruff::settings::configuration::Configuration;
use ::ruff::settings::types::SerializationFormat;
use ::ruff::settings::{pyproject, Settings};
#[cfg(feature = "update-informer")]
use ::ruff::updates;
use ::ruff::{cache, commands, fs};
use anyhow::Result;
use clap::{CommandFactory, Parser};
use colored::Colorize;
use log::{debug, error};
use notify::{recommended_watcher, RecursiveMode, Watcher};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
use rustpython_ast::Location;
use walkdir::DirEntry;

/// Shim that calls `par_iter` except for wasm because there's no wasm support
/// in rayon yet (there is a shim to be used for the web, but it requires js
/// cooperation) Unfortunately, `ParallelIterator` does not implement `Iterator`
/// so the signatures diverge
#[cfg(not(target_family = "wasm"))]
fn par_iter<T: Sync>(iterable: &Vec<T>) -> impl ParallelIterator<Item = &T> {
    iterable.par_iter()
}

#[cfg(target_family = "wasm")]
fn par_iter<T: Sync>(iterable: &Vec<T>) -> impl Iterator<Item = &T> {
    iterable.iter()
}

fn read_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn run_once_stdin(
    settings: &Settings,
    filename: &Path,
    autofix: &fixer::Mode,
) -> Result<Diagnostics> {
    let stdin = read_from_stdin()?;
    let mut diagnostics = lint_stdin(filename, &stdin, settings, autofix)?;
    diagnostics.messages.sort_unstable();
    Ok(diagnostics)
}

fn run_once(
    files: &[PathBuf],
    settings: &Settings,
    cache: bool,
    autofix: &fixer::Mode,
) -> Diagnostics {
    // Collect all the files to check.
    let start = Instant::now();
    let paths: Vec<Result<DirEntry, walkdir::Error>> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude, &settings.extend_exclude))
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let mut diagnostics: Diagnostics = par_iter(&paths)
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
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
                if let Some(path) = path {
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

    diagnostics
}

fn add_noqa(files: &[PathBuf], settings: &Settings) -> usize {
    // Collect all the files to check.
    let start = Instant::now();
    let paths: Vec<DirEntry> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude, &settings.extend_exclude))
        .flatten()
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications: usize = par_iter(&paths)
        .filter_map(|entry| {
            let path = entry.path();
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

    modifications
}

fn autoformat(files: &[PathBuf], settings: &Settings) -> usize {
    // Collect all the files to format.
    let start = Instant::now();
    let paths: Vec<DirEntry> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude, &settings.extend_exclude))
        .flatten()
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications = par_iter(&paths)
        .filter_map(|entry| {
            let path = entry.path();
            match autoformat_path(path) {
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

    modifications
}

fn inner_main() -> Result<ExitCode> {
    // Extract command-line arguments.
    let cli = Cli::parse();
    let fix = cli.fix();
    let log_level = extract_log_level(&cli);
    set_up_logging(&log_level)?;

    if let Some(shell) = cli.generate_shell_completion {
        shell.generate(&mut Cli::command(), &mut std::io::stdout());
        return Ok(ExitCode::SUCCESS);
    }

    // Find the project root and pyproject.toml.
    let config: Option<PathBuf> = cli.config;
    let project_root = config.as_ref().map_or_else(
        || pyproject::find_project_root(&cli.files),
        |config| config.parent().map(fs::normalize_path),
    );
    let pyproject = config.or_else(|| pyproject::find_pyproject_toml(project_root.as_ref()));

    // Reconcile configuration from pyproject.toml and command-line arguments.
    let mut configuration =
        Configuration::from_pyproject(pyproject.as_ref(), project_root.as_ref())?;
    if !cli.exclude.is_empty() {
        configuration.exclude = cli.exclude;
    }
    if !cli.extend_exclude.is_empty() {
        configuration.extend_exclude = cli.extend_exclude;
    }
    if !cli.per_file_ignores.is_empty() {
        configuration.per_file_ignores = collect_per_file_ignores(cli.per_file_ignores);
    }
    if !cli.select.is_empty() {
        configuration.select = cli.select;
    }
    if !cli.extend_select.is_empty() {
        configuration.extend_select = cli.extend_select;
    }
    if !cli.ignore.is_empty() {
        configuration.ignore = cli.ignore;
    }
    if !cli.extend_ignore.is_empty() {
        configuration.extend_ignore = cli.extend_ignore;
    }
    if !cli.fixable.is_empty() {
        configuration.fixable = cli.fixable;
    }
    if !cli.unfixable.is_empty() {
        configuration.unfixable = cli.unfixable;
    }
    if let Some(format) = cli.format {
        configuration.format = format;
    }
    if let Some(line_length) = cli.line_length {
        configuration.line_length = line_length;
    }
    if let Some(max_complexity) = cli.max_complexity {
        configuration.mccabe.max_complexity = max_complexity;
    }
    if let Some(target_version) = cli.target_version {
        configuration.target_version = target_version;
    }
    if let Some(dummy_variable_rgx) = cli.dummy_variable_rgx {
        configuration.dummy_variable_rgx = dummy_variable_rgx;
    }
    if let Some(fix) = fix {
        configuration.fix = fix;
    }
    if cli.show_source {
        configuration.show_source = true;
    }

    if cli.show_settings && cli.show_files {
        eprintln!("Error: specify --show-settings or show-files (not both).");
        return Ok(ExitCode::FAILURE);
    }
    if cli.show_settings {
        commands::show_settings(&configuration, project_root.as_ref(), pyproject.as_ref());
        return Ok(ExitCode::SUCCESS);
    }

    // Extract settings for internal use.
    let autofix = if configuration.fix {
        fixer::Mode::Apply
    } else if matches!(configuration.format, SerializationFormat::Json) {
        fixer::Mode::Generate
    } else {
        fixer::Mode::None
    };
    let settings = Settings::from_configuration(configuration, project_root.as_ref())?;

    // Now that we've inferred the appropriate log level, add some debug
    // information.
    match &project_root {
        Some(path) => debug!("Found project root at: {:?}", path),
        None => debug!("Unable to identify project root; assuming current directory..."),
    };
    match &pyproject {
        Some(path) => debug!("Found pyproject.toml at: {:?}", path),
        None => debug!("Unable to find pyproject.toml; using default settings..."),
    };

    if let Some(code) = cli.explain {
        commands::explain(&code, settings.format)?;
        return Ok(ExitCode::SUCCESS);
    }

    if cli.show_files {
        commands::show_files(&cli.files, &settings);
        return Ok(ExitCode::SUCCESS);
    }

    // Initialize the cache.
    let mut cache_enabled: bool = !cli.no_cache;
    if cache_enabled && cache::init().is_err() {
        eprintln!("Unable to initialize cache; disabling...");
        cache_enabled = false;
    }

    let printer = Printer::new(&settings.format, &log_level);
    if cli.watch {
        if matches!(autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            eprintln!("Warning: --fix is not enabled in watch mode.");
        }
        if cli.add_noqa {
            eprintln!("Warning: --no-qa is not enabled in watch mode.");
        }
        if cli.autoformat {
            eprintln!("Warning: --autoformat is not enabled in watch mode.");
        }
        if settings.format != SerializationFormat::Text {
            eprintln!("Warning: --format 'text' is used in watch mode.");
        }

        // Perform an initial run instantly.
        printer.clear_screen()?;
        printer.write_to_user("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files, &settings, cache_enabled, &fixer::Mode::None);
        printer.write_continuously(&messages)?;

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = recommended_watcher(tx)?;
        for file in &cli.files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        loop {
            match rx.recv() {
                Ok(e) => {
                    let paths = e?.paths;
                    let py_changed = paths.iter().any(|p| {
                        p.extension()
                            .map(|ext| ext.eq_ignore_ascii_case("py"))
                            .unwrap_or_default()
                    });
                    if py_changed {
                        printer.clear_screen()?;
                        printer.write_to_user("File change detected...\n");

                        let messages =
                            run_once(&cli.files, &settings, cache_enabled, &fixer::Mode::None);
                        printer.write_continuously(&messages)?;
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else if cli.add_noqa {
        let modifications = add_noqa(&cli.files, &settings);
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Added {modifications} noqa directives.");
        }
    } else if cli.autoformat {
        let modifications = autoformat(&cli.files, &settings);
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Formatted {modifications} files.");
        }
    } else {
        let is_stdin = cli.files == vec![PathBuf::from("-")];

        // Generate lint violations.
        let diagnostics = if is_stdin {
            let filename = cli.stdin_filename.unwrap_or_else(|| "-".to_string());
            let path = Path::new(&filename);
            run_once_stdin(&settings, path, &autofix)?
        } else {
            run_once(&cli.files, &settings, cache_enabled, &autofix)
        };

        // Always try to print violations (the printer itself may suppress output),
        // unless we're writing fixes via stdin (in which case, the transformed
        // source code goes to stdout).
        if !(is_stdin && matches!(autofix, fixer::Mode::Apply)) {
            printer.write_once(&diagnostics)?;
        }

        // Check for updates if we're in a non-silent log level.
        #[cfg(feature = "update-informer")]
        if !is_stdin && log_level >= LogLevel::Default && atty::is(atty::Stream::Stdout) {
            drop(updates::check_for_updates());
        }

        if !diagnostics.messages.is_empty() && !cli.exit_zero {
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{} {err:?}", "error".red().bold());
            ExitCode::FAILURE
        }
    }
}
