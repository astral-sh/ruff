//! Harvest the Tesseract **textord + ccstruct layout** class manifest — the
//! `ruff>OGAR` structure feeding the layout/line-segmentation transcode
//! (tesseract-rs P3 `pdf-to-text-ocr-v1.md` §Batch 3A, D3.1).
//!
//! Generalizes [`harvest_network`](../harvest_network.rs)'s single-TU-per-header
//! walk over an env-var-driven header list (colon-separated `HEADERS`, or a
//! `HEADER_DIR` directory glob of `*.h`) instead of a hardcoded array — the
//! textord/ directory has 39 headers and the caller shouldn't have to
//! hand-transcribe them.
//!
//! Run (defaults: every `textord/*.h` + the four named `ccstruct/` layout
//! headers + `ccstruct/rect.h`, the last needed because `TBOX` is only
//! forward-declared in `boxread.h`/`polyblk.h`/etc. and fully defined there):
//! ```sh
//! TESSERACT_SRC=/tmp/tesseract LIBCLANG_PATH=/usr/lib/llvm-18/lib \
//!   cargo run -p ruff_cpp_spo --features libclang --example harvest_textord
//! ```
//!
//! Override the header set explicitly:
//! ```sh
//! HEADERS=/tmp/tesseract/src/ccstruct/blobbox.h:/tmp/tesseract/src/ccstruct/rect.h \
//!   cargo run -p ruff_cpp_spo --features libclang --example harvest_textord
//! ```

#![expect(
    clippy::print_stderr,
    reason = "manifest-emission CLI example (mirrors harvest_network)"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ruff_cpp_spo::{CppClass, Declaration, NAMESPACE, model_from_class, walk_tu_with_diagnostics};
use ruff_spo_triplet::{ModelGraph, expand, to_ndjson};

/// Focus classes for the per-class method inventory (the 7 layout primitives
/// named in the batch spec).
const FOCUS: [&str; 7] = [
    "BLOBNBOX",
    "TO_ROW",
    "TO_BLOCK",
    "ROW",
    "BLOCK",
    "TBOX",
    "POLY_BLOCK",
];

fn default_headers(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let textord = root.join("src/textord");
    if let Ok(entries) = std::fs::read_dir(&textord) {
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "h"))
            .collect();
        paths.sort();
        out.extend(paths);
    }
    // The 4 named ccstruct headers, plus rect.h (TBOX's real definition —
    // the 4 named headers only forward-declare it).
    for h in ["blobbox.h", "ocrblock.h", "ocrrow.h", "polyblk.h", "rect.h"] {
        out.push(root.join("src/ccstruct").join(h));
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var("TESSERACT_SRC").unwrap_or_else(|_| "/tmp/tesseract".to_string());
    let root = Path::new(&root);
    if !root.join("src/textord").exists() {
        return Err(format!(
            "{}/src/textord not found; set TESSERACT_SRC",
            root.display()
        )
        .into());
    }

    let headers: Vec<PathBuf> = if let Ok(list) = std::env::var("HEADERS") {
        list.split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .collect()
    } else if let Ok(dir) = std::env::var("HEADER_DIR") {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "h"))
            .collect();
        paths.sort();
        paths
    } else {
        default_headers(root)
    };

    // Every include root a textord/ccstruct/ccutil/arch TU might need.
    // libclang tolerates unresolved leptonica/generated includes; it still
    // surfaces the class declarations for the headers we CAN resolve.
    //
    // `src/viewer` is REQUIRED, not optional: `ccstruct/quspline.h` (pulled in
    // transitively via ocrrow.h -> blobbox.h -> ...) includes "scrollview.h"
    // for its GRAPHICS_DISABLED-gated plot()/plotline() declarations, and
    // scrollview.h lives in src/viewer/. Without it, libclang still reports
    // "0 failed" (the TU itself parses — clang recovers from the unresolved
    // include by treating the rest of the file as best-effort) while the
    // class/method that needed the missing header silently never completes
    // and simply vanishes from the output — confirmed on `STATS`
    // (ccstruct/statistc.h) and, more insidiously, as a corruption of the
    // in-TU call graph for otherwise-healthy classes/functions whose bodies
    // reference a type from the broken chain (see `call_callee_name` in
    // `clang_walker.rs`). See `walk_tu_with_diagnostics` below for the
    // caller-facing visibility into this failure mode.
    let args = [
        "-std=c++17".to_string(),
        "-x".to_string(),
        "c++".to_string(),
        format!("-I{}", root.join("src/textord").display()),
        format!("-I{}", root.join("src/ccstruct").display()),
        format!("-I{}", root.join("src/ccutil").display()),
        format!("-I{}", root.join("src/arch").display()),
        format!("-I{}", root.join("src/ccmain").display()),
        format!("-I{}", root.join("src/api").display()),
        format!("-I{}", root.join("src/dict").display()),
        format!("-I{}", root.join("src/classify").display()),
        format!("-I{}", root.join("src/wordrec").display()),
        format!("-I{}", root.join("src/lstm").display()),
        format!("-I{}", root.join("src/viewer").display()),
        format!("-I{}", root.join("include").display()),
        "-I/usr/include/leptonica".to_string(),
    ];

    let mut all: Vec<CppClass> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    // Headers where the parse itself succeeded ("0 failed") but libclang
    // still reported severity>=Error diagnostics — the silent-drop signature
    // (see the `src/viewer` comment above). Tracked separately from `failed`
    // because `walk_tu_with_diagnostics` still returns a (possibly partial)
    // class list here, unlike a hard `WalkError`.
    let mut warned: Vec<(String, usize)> = Vec::new();
    for h in &headers {
        if !h.exists() {
            eprintln!("[harvest] skip missing {}", h.display());
            continue;
        }
        match walk_tu_with_diagnostics(h, &args) {
            Ok((classes, diagnostics)) => {
                for c in classes {
                    if seen.insert(c.qualified_name()) {
                        all.push(c);
                    }
                }
                if !diagnostics.is_empty() {
                    eprintln!(
                        "[harvest] WARNING: {} unresolved-include error(s) in {} — class list may be incomplete",
                        diagnostics.len(),
                        h.display()
                    );
                    for d in diagnostics.iter().take(5) {
                        eprintln!("    {d}");
                    }
                    warned.push((h.display().to_string(), diagnostics.len()));
                }
            }
            Err(e) => {
                eprintln!("[harvest] walk {} failed: {e}", h.display());
                failed.push((h.display().to_string(), e.to_string()));
            }
        }
    }
    eprintln!(
        "[harvest] {} unique classes across {} headers ({} failed, {} with unresolved-include diagnostics)",
        all.len(),
        headers.len(),
        failed.len(),
        warned.len(),
    );

    eprintln!("\n[textord] focus-class method manifest:");
    for t in FOCUS {
        if let Some(c) = all.iter().find(|c| c.name == t) {
            let methods: Vec<&_> = c
                .declarations
                .iter()
                .filter_map(|d| match d {
                    Declaration::Method(m) => Some(m),
                    _ => None,
                })
                .collect();
            let bases: Vec<&str> = c
                .declarations
                .iter()
                .filter_map(|d| match d {
                    Declaration::Base(b) => Some(b.name.as_str()),
                    _ => None,
                })
                .collect();
            let overrides: Vec<&String> = methods
                .iter()
                .filter(|m| m.overrides.is_some())
                .map(|m| &m.name)
                .collect();
            eprintln!(
                "  {t:12} {:3} methods  bases={bases:?}  overrides={overrides:?}",
                methods.len(),
            );
        } else {
            eprintln!("  {t:12} NOT FOUND");
        }
    }

    if !failed.is_empty() {
        eprintln!("\n[harvest] failed headers:");
        for (h, e) in &failed {
            eprintln!("  {h}: {e}");
        }
    }

    if !warned.is_empty() {
        eprintln!(
            "\n[harvest] headers with unresolved-include diagnostics (class list may be incomplete):"
        );
        for (h, n) in &warned {
            eprintln!("  {h}: {n} error(s)");
        }
    }

    let mut graph = ModelGraph::new(NAMESPACE);
    for c in &all {
        graph.models.push(model_from_class(c));
    }
    let triples = expand(&graph);
    let ndjson = to_ndjson(&triples);
    let out = std::env::var("MANIFEST_OUT")
        .unwrap_or_else(|_| "/tmp/textord_manifest.ndjson".to_string());
    std::fs::write(&out, &ndjson)?;
    eprintln!(
        "\n[harvest] {} models -> {} triples, {} ndjson bytes -> {out}",
        graph.models.len(),
        triples.len(),
        ndjson.len()
    );
    Ok(())
}
