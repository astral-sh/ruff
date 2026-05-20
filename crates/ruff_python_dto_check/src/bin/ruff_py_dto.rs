//! `ruff-py-dto` — CLI driver for the `ruff_python_dto_check` crate.
//!
//! ```text
//! ruff-py-dto harvest    --config <path> [--out <dir>] [--root <override>]
//! ruff-py-dto preflight  <root> [--out <dir>]
//! ruff-py-dto harvest-one --config <path> --rel <repo-rel> <file.py>
//! ruff-py-dto schema     [--out <path>]
//! ```

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use ruff_python_dto_check::harvest_module;

#[derive(Parser)]
#[command(
    name = "ruff-py-dto",
    about = "Config-driven Python AST fact extractor (uses ruff_python_parser).",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Walk a Python repo root, emit per-family ndjson bundles under <out>.
    Harvest {
        /// Path to the JSON config file.
        #[arg(long)]
        config: PathBuf,
        /// Output directory (default: ./ruff-py-dto-out).
        #[arg(long, default_value = "ruff-py-dto-out")]
        out: PathBuf,
        /// Override the root directory from the config.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Scan a tree and emit a proposed config + structured report.
    Preflight {
        /// Python repo root to scan.
        root: PathBuf,
        /// Output directory (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Harvest a single file and print bundles as JSON to stdout.
    HarvestOne {
        /// Path to the JSON config file.
        #[arg(long)]
        config: PathBuf,
        /// Repository-relative path for the bundle's `source.file`.
        #[arg(long, default_value = "stdin.py")]
        rel: String,
        /// Python file to read.
        file: PathBuf,
    },
    /// Write the JSON Schema to <path> or stdout.
    Schema {
        /// Output path (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Harvest { config, out, root } => run_harvest(&config, &out, root.as_deref()),
        Command::Preflight { root, out } => run_preflight(&root, out.as_deref()),
        Command::HarvestOne { config, rel, file } => run_harvest_one(&config, &rel, &file),
        Command::Schema { out } => run_schema(out.as_deref()),
    }
}

fn run_harvest(config_path: &std::path::Path, out: &std::path::Path, _root: Option<&std::path::Path>) -> Result<()> {
    use ruff_python_dto_check::config::Config;
    use ruff_python_dto_check::matcher::function_with_decorator::harvest_module_with_config;
    use ruff_python_dto_check::bundle::write_family_bundles;

    let cfg = Config::from_path(config_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    std::fs::create_dir_all(out)?;

    let root_str = _root
        .and_then(|p| p.to_str())
        .or_else(|| cfg.root.as_deref())
        .unwrap_or(".");
    let root = std::path::Path::new(root_str);

    // Collect all matches across the tree.
    let mut family_map: std::collections::BTreeMap<String, Vec<ruff_python_dto_check::bundle::EmittedBundle>> =
        std::collections::BTreeMap::new();

    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let bundles = harvest_module_with_config(&rel, &source, &cfg);
        for b in bundles {
            family_map.entry(b.family.clone()).or_default().push(b);
        }
    }

    // Sort each family by function_name ascending.
    for bundles in family_map.values_mut() {
        bundles.sort_by(|a, b| a.function_name.cmp(&b.function_name));
    }

    write_family_bundles(&family_map, out)?;

    Ok(())
}

fn run_preflight(root: &std::path::Path, out: Option<&std::path::Path>) -> Result<()> {
    use ruff_python_dto_check::preflight::scanner::PreflightScanner;
    use ruff_python_dto_check::preflight::run_preflight as do_preflight;

    let scanner = PreflightScanner::scan(root)?;
    do_preflight(&scanner, out)?;
    Ok(())
}

fn run_harvest_one(config_path: &std::path::Path, rel: &str, file: &std::path::Path) -> Result<()> {
    use ruff_python_dto_check::config::Config;
    use ruff_python_dto_check::matcher::function_with_decorator::harvest_module_with_config;

    let cfg = Config::from_path(config_path)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let source = std::fs::read_to_string(file)?;
    let bundles = harvest_module_with_config(rel, &source, &cfg);
    let json = serde_json::to_string_pretty(&bundles)?;
    // Binary output to stdout is expected behavior for harvest-one subcommand.
    #[expect(clippy::print_stdout, reason = "harvest-one is a stdout-emit subcommand")]
    {
        println!("{json}");
    }
    Ok(())
}

fn run_schema(out: Option<&std::path::Path>) -> Result<()> {
    let schema = include_str!("../../schemas/ruff-py-dto.config.schema.json");
    if let Some(path) = out {
        std::fs::write(path, schema)?;
    } else {
        // Schema subcommand emits to stdout by design.
        #[expect(clippy::print_stdout, reason = "schema subcommand emits to stdout by design")]
        {
            print!("{schema}");
        }
    }
    Ok(())
}
