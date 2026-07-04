//! Harvest the Tesseract LSTM **network** method-resolution manifest from real
//! Tesseract source — the `ruff>OGAR` sink-in for the recognizer transcode.
//!
//! Walks the network layer headers (`network.h` + the `FullyConnected`/`LSTM`/
//! `Series`/`Parallel`/… subclasses) via libclang and emits the SPO manifest
//! (`has_function` / `inherits_from` / `virtually_overrides`) that the
//! `classid → ClassView` dispatch (`invoke_network`, the `invoke_unicharset`
//! analog) resolves against. Bodies are hand-ported (the doctrine's 15%); this
//! harvest is the minted structure (the 85% — the dispatch table).
//!
//! Writes ndjson to `MANIFEST_OUT` (default `/tmp/network_manifest.ndjson`) and
//! prints, per network class, the method count + the `virtually_overrides` set
//! (the vtable the enum would have hand-rolled) to stderr.
//!
//! Run:
//! ```sh
//! TESSERACT_SRC=/tmp/tesseract LIBCLANG_PATH=/usr/lib/llvm-18/lib \
//!   cargo run -p ruff_cpp_spo --features libclang --example harvest_network
//! ```

#![expect(
    clippy::print_stderr,
    reason = "manifest-emission CLI example (mirrors harvest_unicharset)"
)]

use std::collections::BTreeSet;
use std::path::Path;

use ruff_cpp_spo::{CppClass, Declaration, NAMESPACE, model_from_class, walk_tu};
use ruff_spo_triplet::{ModelGraph, expand, to_ndjson};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var("TESSERACT_SRC").unwrap_or_else(|_| "/tmp/tesseract".to_string());
    let root = Path::new(&root);
    let lstm = root.join("src/lstm");
    if !lstm.join("network.h").exists() {
        return Err(format!("{} not found; set TESSERACT_SRC", lstm.display()).into());
    }

    // Tolerate unresolved generated/leptonica includes (libclang still surfaces
    // the class declarations); supply the ccutil/arch/ccstruct include roots.
    let args = [
        "-std=c++17".to_string(),
        "-x".to_string(),
        "c++".to_string(),
        format!("-I{}", root.join("src/lstm").display()),
        format!("-I{}", root.join("src/arch").display()),
        format!("-I{}", root.join("src/ccstruct").display()),
        format!("-I{}", root.join("src/ccutil").display()),
        format!("-I{}", root.join("include").display()),
    ];

    // The network layer headers (each declares one concrete Network subclass).
    let headers = [
        "network.h",
        "fullyconnected.h",
        "lstm.h",
        "series.h",
        "parallel.h",
        "plumbing.h",
        "convolve.h",
        "maxpool.h",
        "reversed.h",
        "reconfig.h",
        "input.h",
    ];

    let mut all: Vec<CppClass> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for h in headers {
        let path = lstm.join(h);
        if !path.exists() {
            eprintln!("[harvest] skip missing {h}");
            continue;
        }
        match walk_tu(&path, &args) {
            Ok(classes) => {
                for c in classes {
                    if seen.insert(c.qualified_name()) {
                        all.push(c);
                    }
                }
            }
            Err(e) => eprintln!("[harvest] walk {h} failed: {e}"),
        }
    }
    eprintln!("[harvest] {} unique classes across {} headers", all.len(), headers.len());

    // The network class hierarchy — the classid → ClassView method-resolution
    // manifest (what the C++ vtable resolved by type; what invoke_network
    // resolves by classid).
    let targets = [
        "Network",
        "FullyConnected",
        "LSTM",
        "Plumbing",
        "Series",
        "Parallel",
        "Convolve",
        "Maxpool",
        "Reversed",
        "Reconfig",
        "Input",
    ];
    eprintln!("\n[network] classid -> ClassView method manifest:");
    for t in targets {
        if let Some(c) = all.iter().find(|c| c.name == t) {
            let methods: Vec<&_> = c
                .declarations
                .iter()
                .filter_map(|d| match d {
                    Declaration::Method(m) => Some(m),
                    _ => None,
                })
                .collect();
            let overrides: Vec<&String> = methods
                .iter()
                .filter(|m| m.overrides.is_some())
                .map(|m| &m.name)
                .collect();
            eprintln!(
                "  {t:14} {:2} methods  overrides={:?}",
                methods.len(),
                overrides
            );
        } else {
            eprintln!("  {t:14} NOT FOUND");
        }
    }

    // Emit the full ndjson manifest — what lance-graph's SPO store + the
    // tesseract-rs codegen consume.
    let mut graph = ModelGraph::new(NAMESPACE);
    for c in &all {
        graph.models.push(model_from_class(c));
    }
    let triples = expand(&graph);
    let ndjson = to_ndjson(&triples);
    let out = std::env::var("MANIFEST_OUT")
        .unwrap_or_else(|_| "/tmp/network_manifest.ndjson".to_string());
    std::fs::write(&out, &ndjson)?;
    eprintln!(
        "\n[harvest] {} models -> {} triples, {} ndjson bytes -> {out}",
        graph.models.len(),
        triples.len(),
        ndjson.len()
    );
    Ok(())
}
