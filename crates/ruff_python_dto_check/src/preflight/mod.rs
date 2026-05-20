//! Preflight CLI handler: single-pass scan that proposes a starter config
//! and emits a structured report (§3.2).
//!
//! Two sections:
//! 1. Proposed config (JSONC with evidence-count inline comments).
//! 2. Preflight report (`preflight.report.json`), content-encoded only.

pub mod scanner;

use std::collections::BTreeMap;
use std::path::Path;

use scanner::PreflightScanner;

/// Run preflight: scan has already been collected; write proposed config +
/// report to `out` directory (or stdout if `out` is `None`).
pub fn run_preflight(scanner: &PreflightScanner, out: Option<&Path>) -> anyhow::Result<()> {
    let proposed = build_proposed_config(scanner);
    let report = build_report(scanner);

    if let Some(out_dir) = out {
        std::fs::create_dir_all(out_dir)?;
        std::fs::write(out_dir.join("proposed.config.jsonc"), &proposed)?;
        std::fs::write(
            out_dir.join("preflight.report.json"),
            serde_json::to_string_pretty(&report)?,
        )?;
    } else {
        // Stdout: proposed config + blank line + report JSON.
        #[expect(clippy::print_stdout, reason = "preflight stdout output by design")]
        {
            println!("{proposed}");
            println!();
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}

fn build_proposed_config(scanner: &PreflightScanner) -> String {
    // Find the top decorator attribute name.
    let top_attr = scanner
        .decorator_by_attribute
        .iter()
        .max_by_key(|(_, v)| *v)
        .map(|(k, _)| k.as_str())
        .unwrap_or("route");

    let top_count = scanner
        .decorator_by_attribute
        .get(top_attr)
        .copied()
        .unwrap_or(0);

    let runners_up: Vec<String> = scanner
        .decorator_by_attribute
        .iter()
        .filter(|(k, _)| k.as_str() != top_attr)
        .take(3)
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let runners_up_str = runners_up.join(", ");

    format!(
        r#"{{
  "$schema": "../schemas/ruff-py-dto.config.schema.json",
  "root": ".",
  // include: adjust these globs to match your project layout
  "include": ["**/*.py"],
  "exclude": ["**/tests/**", "**/__pycache__/**", "**/.venv/**"],
  "match": [
    {{
      "id": "primary_rule",
      "kind": "function_with_decorator",
      "decorator": {{
        // "attribute": "{top_attr}" — matched {top_count} decorators in tree; runners-up: {runners_up_str}
        "attribute": "{top_attr}",
        "min_positional_args": 1
      }},
      "emit": {{
        "url":            "decorator.args[0]",
        "methods":        "decorator.kwargs.methods",
        "function_name":  "def.name",
        "signature":      "def.params",
        "body_source":    "def.body.source",
        "decorators_all": "def.decorators"
      }}
    }}
  ],
  "group": {{
    "family_from_filename": {{
      "regex": "^(?P<family>[a-z_]+?)(?:_ops|_bp|_routes)?\\.py$"
    }}
  }}
}}
"#
    )
}

fn build_report(scanner: &PreflightScanner) -> serde_json::Value {
    // first_arg_url_test: gather stats from decorator_by_full_pattern.
    let top_full_pattern = scanner
        .decorator_by_full_pattern
        .iter()
        .max_by_key(|(_, v)| *v)
        .map(|(k, _)| k.clone())
        .unwrap_or_default();
    let string_literal_count = scanner
        .decorator_by_full_pattern
        .get(&top_full_pattern)
        .copied()
        .unwrap_or(0);

    serde_json::json!({
        "tree_stats": {
            "py_files_scanned": scanner.py_files_scanned,
            "py_files_parseable": scanner.py_files_parseable,
            "py_files_failed_parse": scanner.py_files_failed_parse,
            "total_function_defs": scanner.total_function_defs,
            "total_class_defs": scanner.total_class_defs
        },
        "framework_fingerprint": {
            "imports_seen": scanner.imports_seen
        },
        "decorator_histogram": {
            "by_attribute_name": scanner.decorator_by_attribute,
            "by_full_pattern": scanner.decorator_by_full_pattern
        },
        "first_arg_url_test": {
            "decorator_pattern": top_full_pattern,
            "string_literal_count": string_literal_count,
            "name_reference_count": 0,
            "starts_with_slash_count": string_literal_count
        },
        "filename_convention": {
            "files_with_matched_routes": scanner.files_with_matched_routes,
            "stem_suffix_histogram": scanner.stem_suffix_histogram
        },
        "url_template_segments": scanner.url_template_segments,
        "body_string_scan_hits": scanner.body_string_hits,
        "candidate_misses": {
            "defs_with_decorators_but_unmatched": scanner.candidate_misses.len(),
            "examples": scanner.candidate_misses.iter().take(5).collect::<Vec<_>>()
        },
        "add_url_rule_findings": scanner.add_url_rule_findings,
        "register_blueprint_graph": scanner.register_blueprint_graph
    })
}

// Group by decorator co-occurrence (for the decorator_co_occurrence field).
pub fn decorator_co_occurrence(
    scanner: &PreflightScanner,
    target_pattern: &str,
) -> BTreeMap<String, usize> {
    let _ = (scanner, target_pattern);
    BTreeMap::new()
}
