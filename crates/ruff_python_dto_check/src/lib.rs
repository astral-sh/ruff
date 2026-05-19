//! `ruff_python_dto_check` — config-driven extractor over
//! `ruff_python_parser` that harvests structured DTO/route/handler facts
//! from a Python source tree into JSON bundles, with a preflight subcommand
//! that proposes a config from the tree itself.
//!
//! This crate is **additive** to ruff: it depends on `ruff_python_parser`,
//! `ruff_python_ast`, and `ruff_source_file` but does not modify any other
//! crate. Ruff and ty continue to work unchanged.
//!
//! Originally landed as `woa_transcode_harvest` (Flask-only, WoA-shaped).
//! Renamed and being generalized so any Python project (and any reading
//! Claude Code session) can point it at a tree and get useful bundles.
//!
//! Reference design RFC: `AdaWorldAPI/woa-rs:rfcs/v02-005-ruff-transcode-harvester.md`.

pub mod bundle;
pub mod extractors;

use std::path::Path;

use anyhow::{Context, Result};
use ruff_python_ast::{Decorator, Stmt, StmtFunctionDef};
use ruff_python_parser::parse_module;
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;

pub use bundle::{Bundle, Decorator as BundleDecorator, Harvester, Source};

/// Schema version. Bumped when the JSON shape changes in a
/// non-backwards-compatible way.
pub const SCHEMA_VERSION: u32 = 1;

/// Harvester version. Bumped on every release of this crate.
pub const HARVESTER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result of harvesting a single Python source file.
#[derive(Debug, Default)]
pub struct ModuleHarvest {
    /// Repository-relative path of the source file (e.g.
    /// `woa/blueprints/vorgaenge_ops.py`).
    pub source_file: String,
    /// One bundle per route detected in the module.
    pub bundles: Vec<Bundle>,
}

/// Parse one Python source file and emit one [`Bundle`] per detected route.
///
/// Detection is currently scoped to top-level functions decorated with a
/// route registrar — `@bp.route(...)`, `@app.route(...)`, or
/// `@<blueprint>.route(...)`. Methods inside classes are not yet harvested
/// (WoA uses module-level routes; classes appear in `models.py` only).
pub fn harvest_module(source_file: &str, source: &str) -> Result<ModuleHarvest> {
    let parsed =
        parse_module(source).with_context(|| format!("parsing {source_file}"))?;
    let line_index = LineIndex::from_source_text(source);

    let mut bundles = Vec::new();
    for stmt in &parsed.syntax().body {
        if let Stmt::FunctionDef(func) = stmt
            && let Some(route) = extractors::routes::detect_route(func)
        {
            bundles.push(build_bundle(source_file, source, &line_index, func, route));
        }
    }

    Ok(ModuleHarvest {
        source_file: source_file.to_string(),
        bundles,
    })
}

/// Harvest every `.py` file under `root`, returning one [`ModuleHarvest`]
/// per file (including those with zero routes; callers can filter).
pub fn harvest_tree(root: &Path) -> Result<Vec<ModuleHarvest>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() || path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        // Skip the obvious ignore-this directories.
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        if rel.contains(".archive/")
            || rel.contains("__pycache__/")
            || rel.starts_with(".claude/")
            || rel.starts_with("tests/")
            || rel.starts_with("venv/")
            || rel.starts_with(".venv/")
        {
            continue;
        }
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        // Skip on parse error; the orchestrator's existing Python AST tools
        // already report syntactic breakage. We only emit clean bundles.
        let Ok(harvest) = harvest_module(&rel, &source) else {
            continue;
        };
        if !harvest.bundles.is_empty() {
            out.push(harvest);
        }
    }
    Ok(out)
}

fn build_bundle(
    source_file: &str,
    source: &str,
    line_index: &LineIndex,
    func: &StmtFunctionDef,
    route: extractors::routes::RouteInfo,
) -> Bundle {
    let body_range = body_range_including_decorators(func);
    let body_text = &source[body_range.start().to_usize()..body_range.end().to_usize()];
    let line_start = line_index.line_index(body_range.start()).get();
    let line_end = line_index.line_index(body_range.end()).get();

    let decorators: Vec<BundleDecorator> = func
        .decorator_list
        .iter()
        .map(|d| decorator_raw(source, d))
        .collect();

    let function = func.name.id.to_string();
    let endpoint = format!("{}.{}", route.blueprint, function);

    Bundle {
        schema_version: SCHEMA_VERSION,
        harvester: Harvester::new(),
        endpoint,
        path: route.path,
        methods: route.methods,
        function,
        family: family_from_path(source_file),
        action: extractors::routes::infer_action(&route.methods_for_action),
        source: Source {
            file: source_file.to_string(),
            line_start: u32::try_from(line_start).unwrap_or(0),
            line_end: u32::try_from(line_end).unwrap_or(0),
            blueprint: route.blueprint,
        },
        phase: None,
        complexity_score: None,
        body_loc: None,
        body: body_text.to_string(),
        body_sha256: String::new(),
        decorators,
    }
}

fn body_range_including_decorators(func: &StmtFunctionDef) -> ruff_text_size::TextRange {
    let start = func
        .decorator_list
        .first()
        .map(Ranged::range)
        .unwrap_or_else(|| func.range())
        .start();
    let end = func.range().end();
    ruff_text_size::TextRange::new(start, end)
}

fn decorator_raw(source: &str, d: &Decorator) -> BundleDecorator {
    let raw = &source[d.range().start().to_usize()..d.range().end().to_usize()];
    BundleDecorator {
        raw: raw.to_string(),
        kind: extractors::decorators::classify(raw),
    }
}

fn family_from_path(source_file: &str) -> String {
    // `woa/blueprints/vorgaenge_ops.py` -> "vorgaenge_ops"
    Path::new(source_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}
