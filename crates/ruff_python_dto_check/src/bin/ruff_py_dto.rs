//! `ruff-py-dto` — CLI driver for the `ruff_python_dto_check` crate.
//!
//! Usage (current, Flask-harvest profile — generic config path lands next):
//!   ruff-py-dto harvest <python-root> [-o <out-dir>]
//!   ruff-py-dto harvest-one <file.py>     (stdout JSON, no out-dir)

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use ruff_python_dto_check::{harvest_module, harvest_tree};

#[derive(Parser)]
#[command(
    name = "ruff-py-dto",
    about = "Config-driven Python AST fact extractor (uses ruff_python_parser).",
    long_about = "Walks a Python source tree and emits JSON bundles describing structured facts (decorated routes, class-based views, DTO-shaped classes, etc.). Today this binary runs a hardcoded Flask-route extraction profile equivalent to the original woa_transcode_harvest behavior; a generic config-driven path and a preflight subcommand are landing next."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Walk a Python repo root, emit one JSON per matched item under <out>.
    Harvest {
        /// Python repo root.
        root: PathBuf,
        /// Output directory. One subdir per family, one JSON per item.
        #[arg(short, long, default_value = "ruff-py-dto-bundles")]
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
