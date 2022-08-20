use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueHint};
use colored::Colorize;
use log::{debug, error};
use notify::{raw_watcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use walkdir::DirEntry;

use ::rust_python_linter::fs::iter_python_files;
use ::rust_python_linter::linter::check_path;
use ::rust_python_linter::logging::set_up_logging;
use ::rust_python_linter::message::Message;
use ::rust_python_linter::tell_user;
use rust_python_linter::settings::Settings;

#[derive(Debug, Parser)]
#[clap(name = "rust-python-linter")]
#[clap(about = "A bare-bones Python linter written in Rust", long_about = None)]
struct Cli {
    #[clap(parse(from_os_str), value_hint = ValueHint::AnyPath, required = true)]
    files: Vec<PathBuf>,
    #[clap(short, long, action)]
    verbose: bool,
    #[clap(short, long, action)]
    watch: bool,
    #[clap(short, long, action)]
    no_cache: bool,
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    // TODO(charlie): Can we avoid this cast?
    let paths: Vec<&Path> = cli.files.iter().map(PathBuf::as_path).collect();
    let settings = Settings::from_paths(paths)?;

    if cli.watch {
        // Perform an initial run instantly.
        clearscreen::clear()?;
        tell_user!("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files, &settings, !cli.no_cache)?;
        report_continuously(&messages)?;

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
                            report_continuously(&messages)?;
                        }
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        let messages = run_once(&cli.files, &settings, !cli.no_cache)?;
        report_once(&messages)?;
    }

    Ok(())
}
