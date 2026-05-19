//! `woa-transcode` — CLI driver for the `woa_transcode_harvest` crate.
//!
//! Usage:
//!   woa-transcode harvest <WoA-root> [-o <out-dir>]
//!   woa-transcode harvest-one <file.py>     (stdout JSON, no out-dir)

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use woa_transcode_harvest::{harvest_module, harvest_tree};

#[derive(Parser)]
#[command(
    name = "woa-transcode",
    about = "Per-route transcode bundle harvester for WoA -> woa-rs.",
    long_about = "Reads WoA's Python sources via the ruff_python_parser and emits one JSON bundle per Flask route. See AdaWorldAPI/woa-rs:rfcs/v02-005-ruff-transcode-harvester.md."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Walk an entire WoA repo root, emit one JSON per route under <out>.
    Harvest {
        /// WoA repo root (the directory containing app.py / models.py).
        root: PathBuf,
        /// Output directory. One subdir per family, one JSON per route.
        #[arg(short, long, default_value = "woa-transcode-bundles")]
        out: PathBuf,
    },
    /// Harvest a single Python file and print its bundles as JSON to stdout.
    /// Useful for debugging extractor behaviour on one route.
    HarvestOne {
        /// Repository-relative path used for the bundle's `source.file`.
        #[arg(long, default_value = "stdin.py")]
        rel: String,
        /// Python file to read.
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Harvest { root, out } => run_harvest(root, out),
        Command::HarvestOne { rel, file } => run_harvest_one(rel, file),
    }
}

fn run_harvest(root: PathBuf, out: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&out)?;
    let modules = harvest_tree(&root)?;
    let mut total = 0_usize;
    for m in &modules {
        for b in &m.bundles {
            let family_dir = out.join(&b.family);
            std::fs::create_dir_all(&family_dir)?;
            let json = serde_json::to_string_pretty(b)?;
            let file = family_dir.join(format!("{}.json", b.function));
            std::fs::write(&file, json)?;
            total += 1;
        }
    }
    eprintln!(
        "harvested {} bundles across {} modules into {}",
        total,
        modules.len(),
        out.display()
    );
    Ok(())
}

fn run_harvest_one(rel: String, file: PathBuf) -> Result<()> {
    let source = std::fs::read_to_string(&file)?;
    let harvest = harvest_module(&rel, &source)?;
    let json = serde_json::to_string_pretty(&harvest.bundles)?;
    println!("{json}");
    Ok(())
}
