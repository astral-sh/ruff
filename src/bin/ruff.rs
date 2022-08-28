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

use ::ruff::checks::CheckCode;
use ::ruff::fs::iter_python_files;
use ::ruff::linter::check_path;
use ::ruff::logging::set_up_logging;
use ::ruff::message::Message;
use ::ruff::settings::Settings;
use ::ruff::tell_user;

#[derive(Debug, Parser)]
#[clap(name = "ruff")]
#[clap(about = "A Python linter written in Rust", long_about = None)]
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
    /// Disable cache reads.
    #[clap(short, long, action)]
    no_cache: bool,
    /// Comma-separated list of error codes to enable.
    #[clap(long)]
    select: Vec<CheckCode>,
    /// Comma-separated list of error codes to ignore.
    #[clap(long)]
    ignore: Vec<CheckCode>,
}

fn run_once(files: &[PathBuf], settings: &Settings, cache: bool) -> Result<Vec<Message>> {
    // Collect all the files to check.
    let start = Instant::now();
    let files: Vec<DirEntry> = files.iter().flat_map(iter_python_files).collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let messages: Vec<Message> = files
        .par_iter()
        .filter(|entry| {
            !settings
                .exclude
                .iter()
                .any(|exclusion| entry.path().starts_with(exclusion))
        })
        .map(|entry| {
            check_path(entry.path(), settings, &cache.into()).unwrap_or_else(|e| {
                error!("Failed to check {}: {e:?}", entry.path().to_string_lossy());
                vec![]
            })
        })
        .flatten()
        .collect();
    let duration = start.elapsed();
    debug!("Checked files in: {:?}", duration);

    Ok(messages)
}

fn report_once(messages: &[Message]) -> Result<()> {
    println!("Found {} error(s).", messages.len());

    if !messages.is_empty() {
        println!();
        for message in messages {
            println!("{}", message);
        }
    }

    Ok(())
}

fn report_continuously(messages: &[Message]) -> Result<()> {
    tell_user!(
        "Found {} error(s). Watching for file changes.",
        messages.len(),
    );

    if !messages.is_empty() {
        println!();
        for message in messages {
            println!("{}", message);
        }
    }

    Ok(())
}

fn inner_main() -> Result<ExitCode> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    // TODO(charlie): Can we avoid this cast?
    let paths: Vec<&Path> = cli.files.iter().map(PathBuf::as_path).collect();
    let mut settings = Settings::from_paths(paths)?;
    if !cli.select.is_empty() {
        settings.select(&cli.select);
    }
    if !cli.ignore.is_empty() {
        settings.ignore(&cli.ignore);
    }

    if cli.watch {
        // Perform an initial run instantly.
        clearscreen::clear()?;
        tell_user!("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files, &settings, !cli.no_cache)?;
        if !cli.quiet {
            report_continuously(&messages)?;
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
                            clearscreen::clear()?;
                            tell_user!("File change detected...\n");

                            let messages = run_once(&cli.files, &settings, !cli.no_cache)?;
                            if !cli.quiet {
                                report_continuously(&messages)?;
                            }
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        let messages = run_once(&cli.files, &settings, !cli.no_cache)?;
        if !cli.quiet {
            report_once(&messages)?;
        }

        if !messages.is_empty() && !cli.exit_zero {
            return Ok(ExitCode::FAILURE);
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match inner_main() {
        Ok(code) => code,
        Err(_) => ExitCode::FAILURE,
    }
}
