//! Harvest the `UNICHARSET` method-resolution manifest from real Tesseract source.
//!
//! This is the first concrete artifact the v1 `PROBE-OGAR-ADAPTER-UNICHARSET`
//! consumes (see `lance-graph` `core-first-transcode-doctrine.md`): UNICHARSET's
//! `has_function` / `virtually_overrides` manifest, from which the Core-First
//! transcode picks the *mechanical, data-shaped leaf* methods (e.g.
//! `unichar_to_id` / `id_to_unichar`) to shape as thin classid-keyed adapters.
//!
//! It writes the full SPO ndjson for `unicharset.h` to `MANIFEST_OUT`
//! (default `/tmp/unicharset_manifest.ndjson`) and prints a human summary of the
//! `UNICHARSET` class's methods + their C++ flags to stderr.
//!
//! Run:
//! ```sh
//! TESSERACT_SRC=/tmp/tesseract LIBCLANG_PATH=/usr/lib/llvm-18/lib \
//!   cargo run -p ruff_cpp_spo --features libclang --example harvest_unicharset
//! ```
//!
//! `TESSERACT_SRC` defaults to `/tmp/tesseract`; the corpus stays UPSTREAM and
//! is never vendored (iron rule of the harvester family).

#![expect(
    clippy::print_stderr,
    reason = "manifest-emission CLI example (mirrors the gated real-corpus test)"
)]

use std::path::Path;

use ruff_cpp_spo::{Declaration, NAMESPACE, model_from_class, walk_tu};
use ruff_spo_triplet::{ModelGraph, expand, to_ndjson};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var("TESSERACT_SRC").unwrap_or_else(|_| "/tmp/tesseract".to_string());
    let root = Path::new(&root);
    let header = root.join("src/ccutil/unicharset.h");
    if !header.exists() {
        return Err(format!("{} not found; set TESSERACT_SRC", header.display()).into());
    }

    // The proven invocation from the real-corpus smoke test: tolerate the
    // unresolved generated/leptonica includes — libclang still surfaces the
    // class declarations we need for the manifest.
    let args = [
        "-std=c++17".to_string(),
        "-x".to_string(),
        "c++".to_string(),
        format!("-I{}", root.join("src/ccutil").display()),
        format!("-I{}", root.join("include").display()),
    ];

    let classes = walk_tu(&header, &args)?;
    eprintln!(
        "[harvest] {} classes from {}",
        classes.len(),
        header.display()
    );

    // The manifest of interest: the `tesseract::UNICHARSET` class's methods
    // (matched by unqualified name — it lives in the `tesseract` namespace).
    if let Some(uni) = classes.iter().find(|c| c.name == "UNICHARSET") {
        eprintln!("\n[UNICHARSET] method-resolution manifest (has_function):");
        let mut method_count = 0usize;
        for decl in &uni.declarations {
            if let Declaration::Method(m) = decl {
                method_count += 1;
                let mut flags = Vec::new();
                if m.is_pure_virtual {
                    flags.push("pure_virtual".to_string());
                }
                if m.is_noexcept {
                    flags.push("noexcept".to_string());
                }
                if let Some(op) = &m.operator_kind {
                    flags.push(format!("operator={op}"));
                }
                if let Some(ov) = &m.overrides {
                    flags.push(format!("virtually_overrides={ov}"));
                }
                let suffix = if flags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", flags.join(", "))
                };
                eprintln!("  - {}{suffix}", m.name);
            }
        }
        eprintln!("[UNICHARSET] {method_count} methods total");
    } else {
        eprintln!(
            "[harvest] UNICHARSET not found; classes: {:?}",
            classes
                .iter()
                .map(ruff_cpp_spo::CppClass::qualified_name)
                .collect::<Vec<_>>()
        );
    }

    // Emit the full ndjson manifest (exactly what the lance-graph SPO store and
    // the tesseract-rs codegen consume).
    let mut graph = ModelGraph::new(NAMESPACE);
    for c in &classes {
        graph.models.push(model_from_class(c));
    }
    let triples = expand(&graph);
    let ndjson = to_ndjson(&triples);
    let out = std::env::var("MANIFEST_OUT")
        .unwrap_or_else(|_| "/tmp/unicharset_manifest.ndjson".to_string());
    std::fs::write(&out, &ndjson)?;
    eprintln!(
        "\n[harvest] {} models -> {} triples, {} ndjson bytes written to {out}",
        graph.models.len(),
        triples.len(),
        ndjson.len()
    );
    Ok(())
}
