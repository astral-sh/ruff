//! `ruff-py-dto` — CLI driver for the `ruff_python_dto_check` crate.
//!
//! ```text
//! ruff-py-dto harvest    --config <path> [--out <dir>] [--root <override>]
//! ruff-py-dto preflight  <root> [--out <dir>]
//! ruff-py-dto harvest-one --config <path> --rel <repo-rel> <file.py>
//! ruff-py-dto schema     [--out <path>]
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use globset::{Glob, GlobSet, GlobSetBuilder};

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
    /// Generate target source (handlers + views + contracts + calibration)
    /// from a Python tree into a draft directory.
    Codegen {
        /// Path to the harvest/extraction JSON config file (drives family
        /// grouping + route detection).
        #[arg(long)]
        config: PathBuf,
        /// Path to the target spec (TOML or JSON). Omit for the built-in
        /// `rust-axum-seaorm` target.
        #[arg(long)]
        target: Option<PathBuf>,
        /// Root directory of the Python tree (overrides the config root).
        #[arg(long)]
        root: Option<PathBuf>,
        /// Output draft directory.
        #[arg(long, default_value = "ruff-py-dto-codegen")]
        out: PathBuf,
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
        Command::Codegen {
            config,
            target,
            root,
            out,
        } => run_codegen(&config, target.as_deref(), root.as_deref(), &out),
        Command::Schema { out } => run_schema(out.as_deref()),
    }
}

fn run_codegen(
    config_path: &std::path::Path,
    target_path: Option<&std::path::Path>,
    root_override: Option<&std::path::Path>,
    out: &std::path::Path,
) -> Result<()> {
    use ruff_python_dto_check::codegen::pipeline::{family_resolver_from_config, run_codegen_tree};
    use ruff_python_dto_check::codegen::target::TargetSpec;
    use ruff_python_dto_check::config::Config;
    use ruff_python_dto_check::extractors::body::ExtractionProfile;

    let cfg = Config::from_path(config_path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let spec = match target_path {
        Some(p) => TargetSpec::from_path(p)?,
        None => TargetSpec::rust_axum_seaorm(),
    };
    let profile = ExtractionProfile::default();

    let root_str = root_override
        .and_then(|p| p.to_str())
        .or(cfg.root.as_deref())
        .unwrap_or(".");
    let root = std::path::Path::new(root_str);

    let resolver = family_resolver_from_config(&cfg);
    let summary = run_codegen_tree(root, out, &profile, &spec, &resolver)?;

    #[expect(clippy::print_stdout, reason = "codegen summary line by design")]
    {
        println!(
            "codegen: {} routes, {} views, {} diagnostics → {}",
            summary.routes,
            summary.views,
            summary.diagnostics,
            out.display()
        );
    }
    Ok(())
}

fn run_harvest(
    config_path: &std::path::Path,
    out: &std::path::Path,
    root_override: Option<&std::path::Path>,
) -> Result<()> {
    use ruff_python_dto_check::bundle::write_family_bundles;
    use ruff_python_dto_check::config::Config;
    use ruff_python_dto_check::matcher::function_with_decorator::harvest_module_with_config;
    use ruff_python_dto_check::observations::attach_observations;

    let cfg = Config::from_path(config_path).map_err(|e| anyhow::anyhow!("{e}"))?;

    std::fs::create_dir_all(out)?;

    let root_str = root_override
        .and_then(|p| p.to_str())
        .or(cfg.root.as_deref())
        .unwrap_or(".");
    let root = std::path::Path::new(root_str);

    let include_set = build_glob_set(&cfg.include).context("compiling include globs")?;
    let exclude_set = build_glob_set(&cfg.exclude).context("compiling exclude globs")?;

    // Collect all matches across the tree, plus a source map so the
    // observation pass can re-parse for AST hashes and parameter counts.
    let mut family_map: BTreeMap<String, Vec<ruff_python_dto_check::bundle::EmittedBundle>> =
        BTreeMap::new();
    let mut source_map: BTreeMap<String, String> = BTreeMap::new();

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
        if !path_passes_filters(&rel, include_set.as_ref(), exclude_set.as_ref()) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let bundles = harvest_module_with_config(&rel, &source, &cfg);
        if !bundles.is_empty() {
            source_map.insert(rel.clone(), source);
            for b in bundles {
                family_map.entry(b.family.clone()).or_default().push(b);
            }
        }
    }

    // Sort each family by function_name ascending before computing the
    // comparison_within_family block so output ordering matches NDJSON.
    for bundles in family_map.values_mut() {
        bundles.sort_by(|a, b| a.function_name.cmp(&b.function_name));
    }

    attach_observations(&mut family_map, &source_map);

    write_family_bundles(&family_map, out)?;

    Ok(())
}

/// Build a [`GlobSet`] from the patterns, or `None` if `patterns` is empty.
/// Empty list means "no filter" — distinct from an empty set that matches nothing.
fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        builder.add(Glob::new(p).with_context(|| format!("invalid glob: {p}"))?);
    }
    Ok(Some(builder.build()?))
}

fn path_passes_filters(rel: &str, include: Option<&GlobSet>, exclude: Option<&GlobSet>) -> bool {
    if let Some(inc) = include
        && !inc.is_match(rel)
    {
        return false;
    }
    if let Some(exc) = exclude
        && exc.is_match(rel)
    {
        return false;
    }
    true
}

fn run_preflight(root: &std::path::Path, out: Option<&std::path::Path>) -> Result<()> {
    use ruff_python_dto_check::preflight::run_preflight as do_preflight;
    use ruff_python_dto_check::preflight::scanner::PreflightScanner;

    let scanner = PreflightScanner::scan(root)?;
    do_preflight(&scanner, out)?;
    Ok(())
}

fn run_harvest_one(config_path: &std::path::Path, rel: &str, file: &std::path::Path) -> Result<()> {
    use ruff_python_dto_check::config::Config;
    use ruff_python_dto_check::matcher::function_with_decorator::harvest_module_with_config;

    let cfg = Config::from_path(config_path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let source = std::fs::read_to_string(file)?;
    let bundles = harvest_module_with_config(rel, &source, &cfg);
    let json = serde_json::to_string_pretty(&bundles)?;
    // Binary output to stdout is expected behavior for harvest-one subcommand.
    #[expect(
        clippy::print_stdout,
        reason = "harvest-one is a stdout-emit subcommand"
    )]
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
        #[expect(
            clippy::print_stdout,
            reason = "schema subcommand emits to stdout by design"
        )]
        {
            print!("{schema}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_patterns_compile_to_none() {
        assert!(build_glob_set(&[]).unwrap().is_none());
    }

    #[test]
    fn invalid_glob_errors() {
        assert!(build_glob_set(&["[unclosed".to_string()]).is_err());
    }

    #[test]
    fn include_filter_admits_matching_paths() {
        let inc = build_glob_set(&["**/blueprints/**/*.py".to_string()])
            .unwrap()
            .unwrap();
        assert!(path_passes_filters(
            "app/blueprints/orders.py",
            Some(&inc),
            None
        ));
        assert!(!path_passes_filters("app/models.py", Some(&inc), None));
    }

    #[test]
    fn exclude_filter_drops_matching_paths() {
        let exc = build_glob_set(&["**/tests/**".to_string()])
            .unwrap()
            .unwrap();
        assert!(path_passes_filters(
            "app/blueprints/orders.py",
            None,
            Some(&exc)
        ));
        assert!(!path_passes_filters(
            "app/tests/test_orders.py",
            None,
            Some(&exc)
        ));
    }

    #[test]
    fn exclude_wins_over_include() {
        let inc = build_glob_set(&["**/*.py".to_string()]).unwrap().unwrap();
        let exc = build_glob_set(&["**/__pycache__/**".to_string()])
            .unwrap()
            .unwrap();
        assert!(path_passes_filters("app/views.py", Some(&inc), Some(&exc)));
        assert!(!path_passes_filters(
            "app/__pycache__/views.cpython.py",
            Some(&inc),
            Some(&exc)
        ));
    }

    #[test]
    fn no_filters_admits_everything() {
        assert!(path_passes_filters("anywhere/foo.py", None, None));
    }
}
