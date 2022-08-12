use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use ::rust_python_linter::fs::collect_python_files;
use ::rust_python_linter::linter::check_path;
use anyhow::Result;
use clap::{Parser, ValueHint};
use colored::Colorize;
use log::{debug, error};
use notify::{watcher, RecursiveMode, Watcher};
use rayon::prelude::*;
use rust_python_linter::message::Message;
use walkdir::DirEntry;

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
}

#[macro_export]
macro_rules! timestamped_println {
    ($($arg:tt)*) => {
        println!(
            "[{}] {}",
            chrono::Local::now()
                .format("%H:%M:%S %p")
                .to_string()
                .dimmed(),
            format_args!($($arg)*)
        )
    }
}

fn set_up_logging(verbose: bool) -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(if verbose {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .chain(std::io::stdout())
        .apply()
        .map_err(|e| e.into())
}

fn run_once(files: &[PathBuf]) -> Result<Vec<Message>> {
    // Collect all the files to check.
    let start = Instant::now();
    let files: Vec<DirEntry> = files.iter().flat_map(collect_python_files).collect();
    let duration = start.elapsed();
    debug!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let messages: Vec<Message> = files
        .par_iter()
        .map(|entry| {
            check_path(entry.path()).unwrap_or_else(|e| {
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
    timestamped_println!(
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

    if cli.watch {
        // Perform an initial run instantly.
        clearscreen::clear()?;
        timestamped_println!("Starting linter in watch mode...\n");

        let messages = run_once(&cli.files)?;
        report_continuously(&messages)?;

        // Configure the file watcher.
        let (tx, rx) = channel();
        let mut watcher = watcher(tx, Duration::from_secs(2))?;
        for file in &cli.files {
            watcher.watch(file, RecursiveMode::Recursive)?;
        }

        loop {
            match rx.recv() {
                Ok(_) => {
                    // Re-run on all change events.
                    clearscreen::clear()?;
                    timestamped_println!("File change detected...\n");

                    let messages = run_once(&cli.files)?;
                    report_continuously(&messages)?;
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        let messages = run_once(&cli.files)?;
        report_once(&messages)?;
    }

    Ok(())
}
