//! Bundle data model — reshaped per §3.5.
//!
//! `EmittedBundle` replaces the WoA-shaped `Bundle` as the primary output
//! type. The legacy `Bundle` is preserved for the backwards-compatible
//! `harvest_module` API (used by the `wo_list_identity` golden test).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::{HARVESTER_VERSION, SCHEMA_VERSION};

// ---------------------------------------------------------------------------
// Legacy WoA-shaped bundle (kept for golden test compatibility)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Bundle {
    pub schema_version: u32,
    pub harvester: Harvester,

    pub endpoint: String,
    pub path: String,
    pub methods: Vec<String>,
    pub function: String,
    pub family: String,
    pub action: String,
    pub source: Source,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complexity_score: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_loc: Option<u32>,

    pub body: String,
    pub body_sha256: String,

    pub decorators: Vec<Decorator>,
}

#[derive(Debug, Serialize)]
pub struct Harvester {
    pub name: &'static str,
    pub version: &'static str,
    pub schema_version: u32,
}

impl Harvester {
    pub fn new() -> Self {
        Self {
            name: "ruff_python_dto_check",
            version: HARVESTER_VERSION,
            schema_version: SCHEMA_VERSION,
        }
    }
}

impl Default for Harvester {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Serialize)]
pub struct Source {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub blueprint: String,
}

#[derive(Debug, Serialize)]
pub struct Decorator {
    pub raw: String,
    pub kind: DecoratorKind,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum DecoratorKind {
    Route,
    Auth,
    Scope,
    ModuleRequired,
    Other,
}

// ---------------------------------------------------------------------------
// New config-driven EmittedBundle (§3.5)
// ---------------------------------------------------------------------------

/// One emitted record per matched function-with-decorator.
/// Carries the `match_id`, fixed identity fields, the config-driven `fields`
/// map, and an optional `comparison_within_family` block computed post-hoc.
#[derive(Debug, Clone, Serialize)]
pub struct EmittedBundle {
    pub match_id: String,
    pub file: String,
    pub function_name: String,
    pub family: String,
    pub line_start: u32,
    pub line_end: u32,
    /// Body line count: `line_end - line_start + 1`.
    pub body_lines: u32,
    /// All decorator raw strings, in source order.
    pub all_decorators: Vec<String>,
    /// Config-driven emit fields (field name → value).
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
    /// Populated by Phase C after all bundles for the family are collected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_within_family: Option<ComparisonWithinFamily>,
}

/// Content-encoded comparison block (§3.4).
///
/// No advisory English strings. Set algebra + distributions only.
/// Field names that are forbidden anywhere in this struct:
/// `warning`, `issue`, `smell`, `outlier`, `confidence`, `severity`,
/// `recommendation`, `should_*`, `is_too_*`, `looks_*`.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonWithinFamily {
    pub family: String,
    pub family_size: usize,
    /// Decorators appearing on every function in the family.
    pub decorators_family_intersection: Vec<String>,
    /// Decorators on this function not in the family intersection.
    pub self_minus_family_intersection: Vec<String>,
    /// Family-intersection decorators absent from this function.
    pub family_intersection_minus_self: Vec<String>,
    /// Body line count for this function.
    pub body_lines_self: u32,
    /// Distribution of body line counts across the family.
    pub body_lines_family_distribution: Distribution,
    /// Parameter count for this function.
    pub param_count_self: usize,
    /// Distribution of parameter counts across the family.
    pub param_count_family_distribution: Distribution,
    /// SHA-256 hash of the AST structure of this function's body.
    pub ast_hash_self: String,
    /// Other function names in the family that share the same `ast_hash_self`.
    pub ast_hash_family_collisions: Vec<String>,
}

/// Percentile distribution of a numeric measure across a family.
#[derive(Debug, Clone, Serialize)]
pub struct Distribution {
    pub p25: u64,
    pub p50: u64,
    pub p75: u64,
    pub p95: u64,
    pub p99: u64,
    pub max: u64,
}

/// Write per-family ndjson files under `out_dir/bundles/`,
/// indices under `out_dir/indices/`, and `out_dir/manifest.json`.
pub fn write_family_bundles(
    family_map: &BTreeMap<String, Vec<EmittedBundle>>,
    out_dir: &Path,
) -> anyhow::Result<()> {
    let bundles_dir = out_dir.join("bundles");
    let indices_dir = out_dir.join("indices");
    std::fs::create_dir_all(&bundles_dir)?;
    std::fs::create_dir_all(&indices_dir)?;

    let mut by_decorator_stack: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_ast_hash: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_family: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut total_bundles: usize = 0;

    for (family, bundles) in family_map {
        // Write ndjson — one JSON object per line, stable-ordered by function_name.
        let ndjson_path = bundles_dir.join(format!("{family}.ndjson"));
        let mut ndjson = String::new();
        for b in bundles {
            ndjson.push_str(&serde_json::to_string(b)?);
            ndjson.push('\n');
            total_bundles += 1;

            let key = b.function_name.clone();
            by_family.entry(family.clone()).or_default().push(key.clone());

            let dec_sig: Vec<&str> = b.all_decorators.iter().map(String::as_str).collect();
            let dec_sig_key = dec_sig.join("|");
            by_decorator_stack.entry(dec_sig_key).or_default().push(key.clone());

            if let Some(cwf) = &b.comparison_within_family {
                let hash = cwf.ast_hash_self.clone();
                by_ast_hash.entry(hash).or_default().push(key);
            }
        }
        std::fs::write(&ndjson_path, &ndjson)?;
    }

    // Write indices/by_family.json
    let by_family_json = serde_json::to_string_pretty(&by_family)?;
    std::fs::write(indices_dir.join("by_family.json"), by_family_json)?;

    // Write indices/by_decorator_stack.json
    let by_dec_json = serde_json::to_string_pretty(&by_decorator_stack)?;
    std::fs::write(indices_dir.join("by_decorator_stack.json"), by_dec_json)?;

    // Write indices/by_ast_hash.json — groups of size ≥ 2 only
    let filtered_hashes: BTreeMap<&str, &Vec<String>> = by_ast_hash
        .iter()
        .filter(|(_, v)| v.len() >= 2)
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    let by_hash_json = serde_json::to_string_pretty(&filtered_hashes)?;
    std::fs::write(indices_dir.join("by_ast_hash.json"), by_hash_json)?;

    // Write manifest.json
    let manifest = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "ruff_py_dto_version": HARVESTER_VERSION,
        "generated_at": "2026-05-19T00:00:00Z",
        "totals": {
            "families": family_map.len(),
            "bundles": total_bundles
        }
    });
    std::fs::write(out_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;

    Ok(())
}
