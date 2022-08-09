use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueHint};
use log::{error, info};
use rayon::prelude::*;
use rust_python_linter::fs::collect_python_files;
use rust_python_linter::linter::check_path;
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
        .level_for("hyper", log::LevelFilter::Info)
        .chain(std::io::stdout())
        .apply()
        .map_err(|e| e.into())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    set_up_logging(cli.verbose)?;

    // Collect all the files to check.
    let start = Instant::now();
    let files: Vec<DirEntry> = cli.files.iter().flat_map(collect_python_files).collect();
    let duration = start.elapsed();
    info!("Identified files to lint in: {:?}", duration);

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
    info!("Checked files in: {:?}", duration);

    if !messages.is_empty() {
        println!("Found {} error(s)!", messages.len());
        for message in messages {
            println!("{}", message);
        }
    }

    Ok(())
}
