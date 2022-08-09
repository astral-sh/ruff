use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueHint};

use log::info;
use rayon::prelude::*;
use walkdir::{DirEntry, WalkDir};

use rust_python_linter::linter::check_path;
use rust_python_linter::message::Message;

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
        .level_for("hyper", log::LevelFilter::Info)
        .chain(std::io::stdout())
        .apply()
        .map_err(|e| e.into())
}

#[derive(Debug, Parser)]
#[clap(name = "rust-python-linter")]
#[clap(about = "A bare-bones Python linter written in Rust", long_about = None)]
struct Cli {
    #[clap(name = "filename", parse(from_os_str), value_hint = ValueHint::DirPath)]
    filename: PathBuf,
    #[clap(short, long, action)]
    verbose: bool,
    // /// Files to process
    // #[clap(name = "FILE", parse(from_os_str), value_hint = ValueHint::AnyPath)]
    // files: Vec<PathBuf>,
}

fn is_not_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| entry.depth() == 0 || !s.starts_with('.'))
        .unwrap_or(false)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    // Collect all the files to check.
    let start = Instant::now();
    let files: Vec<DirEntry> = WalkDir::new(cli.filename)
        .follow_links(true)
        .into_iter()
        .filter_entry(is_not_hidden)
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().to_string_lossy().ends_with(".py"))
        .collect();
    let duration = start.elapsed();
    info!("Identified files to lint in: {:?}", duration);

    let start = Instant::now();
    let messages: Vec<Message> = files
        .par_iter()
        .map(|entry| check_path(entry.path()).unwrap())
        .flatten()
        .collect();
    let duration = start.elapsed();
    info!("Checked files in: {:?}", duration);

    if !messages.is_empty() {
        println!("Found {} error(s)!", messages.len());
        for message in messages {
            println!("{}", message);
        }
    }

    Ok(())
}
