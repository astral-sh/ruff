extern crate core;

use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueHint};
use colored::Colorize;
use log::{debug, error};
use notify::{raw_watcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use walkdir::DirEntry;

use ::ruff::cache;
use ::ruff::checks::CheckCode;
use ::ruff::checks::CheckKind;
use ::ruff::fs::iter_python_files;
use ::ruff::linter::add_noqa_to_path;
use ::ruff::linter::lint_path;
use ::ruff::logging::set_up_logging;
use ::ruff::message::Message;
use ::ruff::printer::{Printer, SerializationFormat};
use ::ruff::pyproject;
use ::ruff::settings::{FilePattern, Settings};
use ::ruff::tell_user;

const CARGO_PKG_NAME: &str = env!("CARGO_PKG_NAME");
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Parser)]
#[clap(name = format!("{CARGO_PKG_NAME} (v{CARGO_PKG_VERSION})"))]
#[clap(about = "An extremely fast Python linter.", long_about = None)]
#[clap(version)]
struct Cli {
    #[clap(parse(from_os_str), value_hint = ValueHint::AnyPath, required = true)]
    files: Vec<PathBuf>,
    /// Enable verbose logging.
    #[clap(short, long, action)]
    verbose: bool,
    /// Disable all logging (but still exit with status code "1" upon detecting errors).
    #[clap(short, long, action)]
    quiet: bool,
    /// Exit with status code "0", even upon detecting errors.
    #[clap(short, long, action)]
    exit_zero: bool,
    /// Run in watch mode by re-running whenever files change.
    #[clap(short, long, action)]
    watch: bool,
    /// Attempt to automatically fix lint errors.
    #[clap(short, long, action)]
    fix: bool,
    /// Disable cache reads.
    #[clap(short, long, action)]
    no_cache: bool,
    /// List of error codes to enable.
    #[clap(long, multiple = true)]
    select: Vec<CheckCode>,
    /// Like --select, but adds additional error codes on top of the selected ones.
    #[clap(long, multiple = true)]
    extend_select: Vec<CheckCode>,
    /// List of error codes to ignore.
    #[clap(long, multiple = true)]
    ignore: Vec<CheckCode>,
    /// Like --ignore, but adds additional error codes on top of the ignored ones.
    #[clap(long, multiple = true)]
    extend_ignore: Vec<CheckCode>,
    /// List of paths, used to exclude files and/or directories from checks.
    #[clap(long, multiple = true)]
    exclude: Vec<String>,
    /// Like --exclude, but adds additional files and directories on top of the excluded ones.
    #[clap(long, multiple = true)]
    extend_exclude: Vec<String>,
    /// Output serialization format for error messages.
    #[clap(long, arg_enum, default_value_t=SerializationFormat::Text)]
    format: SerializationFormat,
    /// See the files ruff will be run against with the current settings.
    #[clap(long, action)]
    show_files: bool,
    /// See ruff's settings.
    #[clap(long, action)]
    show_settings: bool,
    /// Enable automatic additions of noqa directives to failing lines.
    #[clap(long, action)]
    add_noqa: bool,
}

#[cfg(feature = "update-informer")]
fn check_for_updates() {
    use update_informer::{registry, Check};

    let informer = update_informer::new(registry::PyPI, CARGO_PKG_NAME, CARGO_PKG_VERSION);

    if let Some(new_version) = informer.check_version().ok().flatten() {
        let msg = format!(
            "A new version of {pkg_name} is available: v{pkg_version} -> {new_version}",
            pkg_name = CARGO_PKG_NAME.italic().cyan(),
            pkg_version = CARGO_PKG_VERSION,
            new_version = new_version.to_string().green()
        );

        let cmd = format!(
            "Run to update: {cmd} {pkg_name}",
            cmd = "pip3 install --upgrade".green(),
            pkg_name = CARGO_PKG_NAME.green()
        );

        println!("\n{msg}\n{cmd}");
    }
}

fn show_settings(settings: &Settings) {
    println!("{:#?}", settings);
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
    let mut messages: Vec<Message> = paths
        .par_iter()
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
                    if settings.select.contains(&CheckCode::E902) {
                        vec![Message {
                            kind: CheckKind::IOError(message),
                            fixed: false,
                            location: Default::default(),
                            filename: path.to_string_lossy().to_string(),
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
    let modifications: usize = paths
        .par_iter()
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

fn inner_main() -> Result<ExitCode> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    // Find the project root and pyproject.toml.
    let project_root = pyproject::find_project_root(&cli.files);
    match &project_root {
        Some(path) => debug!("Found project root at: {:?}", path),
        None => debug!("Unable to identify project root; assuming current directory..."),
    };
    let pyproject = pyproject::find_pyproject_toml(&project_root);
    match &pyproject {
        Some(path) => debug!("Found pyproject.toml at: {:?}", path),
        None => debug!("Unable to find pyproject.toml; using default settings..."),
    };

    // Parse the settings from the pyproject.toml and command-line arguments.
    let exclude: Vec<FilePattern> = cli
        .exclude
        .iter()
        .map(|path| FilePattern::from_user(path, &project_root))
        .collect();
    let extend_exclude: Vec<FilePattern> = cli
        .extend_exclude
        .iter()
        .map(|path| FilePattern::from_user(path, &project_root))
        .collect();

    let mut settings = Settings::from_pyproject(pyproject, project_root)?;
    if !exclude.is_empty() {
        settings.exclude = exclude;
    }
    if !extend_exclude.is_empty() {
        settings.extend_exclude = extend_exclude;
    }
    if !cli.select.is_empty() {
        settings.clear();
        settings.select(cli.select);
    }
    if !cli.extend_select.is_empty() {
        settings.select(cli.extend_select);
    }
    if !cli.ignore.is_empty() {
        settings.ignore(&cli.ignore);
    }
    if !cli.extend_ignore.is_empty() {
        settings.ignore(&cli.extend_ignore);
    }

    if cli.show_settings && cli.show_files {
        eprintln!("Error: specify --show-settings or show-files (not both).");
        return Ok(ExitCode::FAILURE);
    }
    if cli.show_settings {
        show_settings(&settings);
        return Ok(ExitCode::SUCCESS);
    }
    if cli.show_files {
        show_files(&cli.files, &settings);
        return Ok(ExitCode::SUCCESS);
    }

    cache::init()?;

    let mut printer = Printer::new(cli.format, cli.verbose);
    if cli.watch {
        if cli.fix {
            eprintln!("Warning: --fix is not enabled in watch mode.");
        }

        if cli.add_noqa {
            eprintln!("Warning: --no-qa is not enabled in watch mode.");
        }

        if cli.format != SerializationFormat::Text {
            eprintln!("Warning: --format 'text' is used in watch mode.");
        }

        // Perform an initial run instantly.
        printer.clear_screen()?;
        tell_user!("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files, &settings, !cli.no_cache, false)?;
        if !cli.quiet {
            printer.write_continuously(&messages)?;
        }

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
                            tell_user!("File change detected...\n");

                            let messages = run_once(&cli.files, &settings, !cli.no_cache, false)?;
                            if !cli.quiet {
                                printer.write_continuously(&messages)?;
                            }
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else if cli.add_noqa {
        let modifications = add_noqa(&cli.files, &settings)?;
        if modifications > 0 {
            println!("Added {modifications} noqa directives.");
        }
    } else {
        let messages = run_once(&cli.files, &settings, !cli.no_cache, cli.fix)?;
        if !cli.quiet {
            printer.write_once(&messages)?;
        }

        #[cfg(feature = "update-informer")]
        check_for_updates();

        if !messages.is_empty() && !cli.exit_zero {
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
