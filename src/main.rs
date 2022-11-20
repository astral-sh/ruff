use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::Instant;

use ::ruff::cache;
use ::ruff::checks::{CheckCode, CheckKind};
use ::ruff::cli::{collect_per_file_ignores, extract_log_level, Cli};
use ::ruff::fs::iter_python_files;
use ::ruff::linter::{add_noqa_to_path, autoformat_path, lint_path, lint_stdin};
use ::ruff::logging::{set_up_logging, LogLevel};
use ::ruff::message::Message;
use ::ruff::printer::{Printer, SerializationFormat};
use ::ruff::settings::configuration::Configuration;
use ::ruff::settings::types::FilePattern;
use ::ruff::settings::user::UserConfiguration;
use ::ruff::settings::{pyproject, Settings};
#[cfg(feature = "update-informer")]
use ::ruff::updates;
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use log::{debug, error};
use notify::{raw_watcher, RecursiveMode, Watcher};
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
use walkdir::DirEntry;

/// Shim that calls par_iter except for wasm because there's no wasm support in
/// rayon yet (there is a shim to be used for the web, but it requires js
/// cooperation) Unfortunately, ParallelIterator does not implement Iterator so
/// the signatures diverge
#[cfg(not(target_family = "wasm"))]
fn par_iter<T: Sync>(iterable: &Vec<T>) -> impl ParallelIterator<Item = &T> {
    iterable.par_iter()
}

#[cfg(target_family = "wasm")]
fn par_iter<T: Sync>(iterable: &Vec<T>) -> impl Iterator<Item = &T> {
    iterable.iter()
}

fn show_settings(
    configuration: Configuration,
    project_root: Option<PathBuf>,
    pyproject: Option<PathBuf>,
) {
    println!(
        "{:#?}",
        UserConfiguration::from_configuration(configuration, project_root, pyproject)
    );
}

fn show_files(files: &[PathBuf], settings: &Settings) {
    let mut entries: Vec<DirEntry> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude, &settings.extend_exclude))
        .flatten()
        .collect();
    entries.sort_by(|a, b| a.path().cmp(b.path()));
    for entry in entries {
        println!("{}", entry.path().to_string_lossy());
    }
}

fn read_from_stdin() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().lock().read_to_string(&mut buffer)?;
    Ok(buffer)
}

fn run_once_stdin(settings: &Settings, filename: &Path, autofix: bool) -> Result<Vec<Message>> {
    let stdin = read_from_stdin()?;
    let mut messages = lint_stdin(filename, &stdin, settings, &autofix.into())?;
    messages.sort_unstable();
    Ok(messages)
}

fn run_once(
    files: &[PathBuf],
    settings: &Settings,
    cache: bool,
    autofix: bool,
) -> Result<Vec<Message>> {
    // Collect all the files to check.
    let start = Instant::now();
    let paths: Vec<Result<DirEntry, walkdir::Error>> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude, &settings.extend_exclude))
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let mut messages: Vec<Message> = par_iter(&paths)
        .map(|entry| {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    lint_path(path, settings, &cache.into(), &autofix.into())
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
                        vec![Message {
                            kind: CheckKind::IOError(message),
                            fixed: false,
                            location: Default::default(),
                            end_location: Default::default(),
                            filename: path.to_string_lossy().to_string(),
                            source: None,
                        }]
                    } else {
                        error!("Failed to check {}: {message}", path.to_string_lossy());
                        vec![]
                    }
                } else {
                    error!("{message}");
                    vec![]
                }
            })
        })
        .flatten()
        .collect();

    messages.sort_unstable();
    let duration = start.elapsed();
    debug!("Checked files in: {:?}", duration);

    Ok(messages)
}

fn add_noqa(files: &[PathBuf], settings: &Settings) -> Result<usize> {
    // Collect all the files to check.
    let start = Instant::now();
    let paths: Vec<Result<DirEntry, walkdir::Error>> = files
        .iter()
        .flat_map(|path| iter_python_files(path, &settings.exclude, &settings.extend_exclude))
        .collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let modifications: usize = par_iter(&paths)
        .map(|entry| match entry {
            Ok(entry) => {
                let path = entry.path();
                add_noqa_to_path(path, settings)
            }
            Err(_) => Ok(0),
        })
        .flatten()
        .sum();

    let duration = start.elapsed();
    debug!("Added noqa to files in: {:?}", duration);

    Ok(modifications)
}

fn autoformat(files: &[PathBuf], settings: &Settings) -> Result<usize> {
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
        .map(|entry| {
            let path = entry.path();
            autoformat_path(path)
        })
        .flatten()
        .count();

    let duration = start.elapsed();
    debug!("Auto-formatted files in: {:?}", duration);

    Ok(modifications)
}

fn inner_main() -> Result<ExitCode> {
    // Extract command-line arguments.
    let cli = Cli::parse();
    let fix = cli.fix();

    let log_level = extract_log_level(&cli);
    set_up_logging(&log_level)?;

    // Find the project root and pyproject.toml.
    let project_root = pyproject::find_project_root(&cli.files);
    match &project_root {
        Some(path) => debug!("Found project root at: {:?}", path),
        None => debug!("Unable to identify project root; assuming current directory..."),
    };
    let pyproject = cli
        .config
        .or_else(|| pyproject::find_pyproject_toml(project_root.as_ref()));
    match &pyproject {
        Some(path) => debug!("Found pyproject.toml at: {:?}", path),
        None => debug!("Unable to find pyproject.toml; using default settings..."),
    };

    // Reconcile configuration from pyproject.toml and command-line arguments.
    let exclude: Vec<FilePattern> = cli
        .exclude
        .iter()
        .map(|path| FilePattern::from_user(path, project_root.as_ref()))
        .collect();
    let extend_exclude: Vec<FilePattern> = cli
        .extend_exclude
        .iter()
        .map(|path| FilePattern::from_user(path, project_root.as_ref()))
        .collect();

    let mut configuration =
        Configuration::from_pyproject(pyproject.as_ref(), project_root.as_ref())?;
    if !exclude.is_empty() {
        configuration.exclude = exclude;
    }
    if !extend_exclude.is_empty() {
        configuration.extend_exclude = extend_exclude;
    }
    if !cli.per_file_ignores.is_empty() {
        configuration.per_file_ignores =
            collect_per_file_ignores(cli.per_file_ignores, project_root.as_ref());
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
        show_settings(configuration, project_root, pyproject);
        return Ok(ExitCode::SUCCESS);
    }

    // Extract settings for internal use.
    let fix_enabled: bool = configuration.fix;
    let settings = Settings::from_configuration(configuration);

    if cli.show_files {
        show_files(&cli.files, &settings);
        return Ok(ExitCode::SUCCESS);
    }

    // Initialize the cache.
    let mut cache_enabled: bool = !cli.no_cache;
    if cache_enabled && cache::init().is_err() {
        eprintln!("Unable to initialize cache; disabling...");
        cache_enabled = false;
    }

    let printer = Printer::new(&cli.format, &log_level);
    if cli.watch {
        if fix_enabled {
            eprintln!("Warning: --fix is not enabled in watch mode.");
        }

        if cli.add_noqa {
            eprintln!("Warning: --no-qa is not enabled in watch mode.");
        }

        if cli.autoformat {
            eprintln!("Warning: --autoformat is not enabled in watch mode.");
        }

        if cli.format != SerializationFormat::Text {
            eprintln!("Warning: --format 'text' is used in watch mode.");
        }

        // Perform an initial run instantly.
        printer.clear_screen()?;
        printer.write_to_user("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files, &settings, cache_enabled, false)?;
        printer.write_continuously(&messages)?;

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = raw_watcher(tx)?;
        for file in &cli.files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        loop {
            match rx.recv() {
                Ok(e) => {
                    if let Some(path) = e.path {
                        if path.to_string_lossy().ends_with(".py") {
                            printer.clear_screen()?;
                            printer.write_to_user("File change detected...\n");

                            let messages = run_once(&cli.files, &settings, cache_enabled, false)?;
                            printer.write_continuously(&messages)?;
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else if cli.add_noqa {
        let modifications = add_noqa(&cli.files, &settings)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Added {modifications} noqa directives.");
        }
    } else if cli.autoformat {
        let modifications = autoformat(&cli.files, &settings)?;
        if modifications > 0 && log_level >= LogLevel::Default {
            println!("Formatted {modifications} files.");
        }
    } else {
        let is_stdin = cli.files == vec![PathBuf::from("-")];

        // Generate lint violations.
        let messages = if is_stdin {
            let filename = cli.stdin_filename.unwrap_or_else(|| "-".to_string());
            let path = Path::new(&filename);
            run_once_stdin(&settings, path, fix_enabled)?
        } else {
            run_once(&cli.files, &settings, cache_enabled, fix_enabled)?
        };

        // Always try to print violations (the printer itself may suppress output),
        // unless we're writing fixes via stdin (in which case, the transformed
        // source code goes to stdout).
        if !(is_stdin && fix_enabled) {
            printer.write_once(&messages)?;
        }

        // Check for updates if we're in a non-silent log level.
        #[cfg(feature = "update-informer")]
        if !is_stdin && log_level >= LogLevel::Default && atty::is(atty::Stream::Stdout) {
            let _ = updates::check_for_updates();
        }

        if messages.iter().any(|message| !message.fixed) && !cli.exit_zero {
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{} {:?}", "error".red().bold(), err);
            ExitCode::FAILURE
        }
    }
}
