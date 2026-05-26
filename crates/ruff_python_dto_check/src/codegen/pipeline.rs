//! End-to-end driver: source tree → contracts → target source + calibration.
//!
//! Ties together the route detector ([`crate::extractors::routes`]), the body
//! extractor, the contract builder/classifier, the target emitter, and the
//! calibration lints. Output goes to a draft directory, idempotently.

use std::path::Path;

use anyhow::{Context, Result};
use ruff_python_ast::{Stmt, StmtFunctionDef};
use ruff_python_parser::parse_module;
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;

use crate::calibrate::{Diagnostic, calibrate, calibration_report};
use crate::codegen::{Emitted, emit};
use crate::codegen::target::TargetSpec;
use crate::contract::{Provenance, RouteContract, build_contract, contract_to_json};
use crate::extractors::body::{ExtractionProfile, extract_body};
use crate::extractors::routes::detect_route;
use crate::matcher::function_with_decorator::resolve_family;

/// All artifacts produced for one route, for in-memory testing.
#[derive(Debug, Clone)]
pub struct RouteOutput {
    pub contract: RouteContract,
    pub emitted: Emitted,
    pub diagnostics: Vec<Diagnostic>,
}

/// Process one source file → a list of [`RouteOutput`].
pub fn process_source(
    source_file: &str,
    source: &str,
    family: &str,
    profile: &ExtractionProfile,
    spec: &TargetSpec,
) -> Vec<RouteOutput> {
    let Ok(parsed) = parse_module(source) else {
        return Vec::new();
    };
    let line_index = LineIndex::from_source_text(source);
    let mut out = Vec::new();

    for stmt in &parsed.syntax().body {
        if let Stmt::FunctionDef(func) = stmt
            && let Some(route) = detect_route(func)
        {
            let output = build_route_output(
                source_file,
                &line_index,
                func,
                &route.blueprint,
                &route.path,
                &route.methods,
                family,
                profile,
                spec,
            );
            out.push(output);
        }
    }
    out
}

#[expect(
    clippy::too_many_arguments,
    reason = "threads route identity + profile + spec; intermediate struct would only relocate the arity"
)]
fn build_route_output(
    source_file: &str,
    line_index: &LineIndex,
    func: &StmtFunctionDef,
    blueprint: &str,
    path: &str,
    methods: &[String],
    family: &str,
    profile: &ExtractionProfile,
    spec: &TargetSpec,
) -> RouteOutput {
    let facts = extract_body(func, profile);
    let line_start =
        u32::try_from(line_index.line_index(func.range().start()).get()).unwrap_or(0);
    let line_end = u32::try_from(line_index.line_index(func.range().end()).get()).unwrap_or(0);
    let provenance = Provenance {
        file: source_file.to_string(),
        line_start,
        line_end,
    };
    let function = func.name.id.to_string();
    let contract = build_contract(
        blueprint, &function, family, methods, path, facts, provenance,
    );
    let emitted = emit(&contract, spec);
    let diagnostics = calibrate(&contract, &emitted, spec, None);
    RouteOutput {
        contract,
        emitted,
        diagnostics,
    }
}

/// Walk a tree and write contract JSON + handlers + views + `calibration.json`
/// under `out`. Idempotent (overwrites). `family_regex` mirrors the harvest
/// family rule when supplied via the config.
pub fn run_codegen_tree(
    root: &Path,
    out: &Path,
    profile: &ExtractionProfile,
    spec: &TargetSpec,
    family_resolver: &dyn Fn(&str) -> String,
) -> Result<CodegenSummary> {
    let contracts_dir = out.join("contracts");
    let handlers_dir = out.join("handlers");
    let views_dir = out.join("views");
    std::fs::create_dir_all(&contracts_dir)?;
    std::fs::create_dir_all(&handlers_dir)?;
    std::fs::create_dir_all(&views_dir)?;

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut summary = CodegenSummary::default();

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
        if rel.contains("__pycache__/") || rel.contains(".venv/") || rel.contains("venv/") {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let family = family_resolver(&rel);
        let outputs = process_source(&rel, &source, &family, profile, spec);
        for ro in outputs {
            summary.routes += 1;
            // Contract JSON.
            let contract_json = serde_json::to_string_pretty(&contract_to_json(&ro.contract))?;
            std::fs::write(
                contracts_dir.join(format!(
                    "{}__{}.json",
                    ro.contract.family, ro.contract.function
                )),
                contract_json,
            )
            .with_context(|| format!("writing contract for {}", ro.contract.id))?;
            // Handler.
            std::fs::write(handlers_dir.join(&ro.emitted.handler_file), &ro.emitted.handler_rs)?;
            // View.
            if let (Some(view), Some(file)) = (&ro.emitted.view_html, &ro.emitted.view_file) {
                std::fs::write(views_dir.join(file), view)?;
                summary.views += 1;
            }
            summary.diagnostics += ro.diagnostics.len();
            all_diagnostics.extend(ro.diagnostics);
        }
    }

    let report = calibration_report(&all_diagnostics);
    std::fs::write(
        out.join("calibration.json"),
        serde_json::to_string_pretty(&report)?,
    )?;

    Ok(summary)
}

/// Summary returned to the CLI.
#[derive(Debug, Default, Clone)]
pub struct CodegenSummary {
    pub routes: usize,
    pub views: usize,
    pub diagnostics: usize,
}

/// Resolve a family name from a relative path using the harvest config's
/// `family_from_filename` rule, falling back to the file stem.
pub fn family_resolver_from_config(
    config: &crate::config::Config,
) -> impl Fn(&str) -> String + '_ {
    move |rel: &str| resolve_family(rel, config)
}
