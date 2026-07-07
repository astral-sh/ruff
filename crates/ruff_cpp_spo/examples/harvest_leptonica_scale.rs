//! Harvest the leptonica **`pixScale` call graph** — the `ruff>OGAR` structure
//! that DRIVES the byte-exact `pixScale` transcode (tesseract-rs image
//! front-end, non-model-height scaling).
//!
//! leptonica is a **C library** (free functions on pointer buffers), so this
//! uses [`walk_free_functions`] — the C-library harvest arm, distinct from the
//! C++-class [`walk_tu`](ruff_cpp_spo::walk_tu). The numeric kernel BODIES
//! (`scaleGrayLILow`, the area-map / unsharp low-levels) are the doctrine's
//! essential-15% hand-port; this manifest is the 85% structure: WHICH functions
//! the transcode must port and in what **dispatch order** (`pixScale →
//! pixScaleGeneral → {pixScaleGrayLI → scaleGrayLILow, pixScaleAreaMap,
//! pixUnsharpMasking}`). Correlating each body to that call graph (fuzzy-recipe
//! codebook §1) is how the transcode stays driven, not eyeballed.
//!
//! Run:
//! ```sh
//! LIBCLANG_PATH=/usr/lib/llvm-18/lib SCALE_SRC=/tmp/leptonica-src/scale1.c \
//!   cargo run -p ruff_cpp_spo --features libclang --example harvest_leptonica_scale
//! ```

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "manifest-emission CLI example (mirrors harvest_network)"
)]

use std::path::Path;

use ruff_cpp_spo::walk_free_functions;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let src = std::env::var("SCALE_SRC").unwrap_or_else(|_| "/tmp/leptonica-src/scale1.c".to_string());
    let inc = std::env::var("LEPT_INCLUDE").unwrap_or_else(|_| "/usr/include/leptonica".to_string());
    // LANG=c++ switches to C++ mode (namespaced free functions, e.g. tesseract's
    // otsuthr.cpp); EXTRA_INC=colon,separated adds include dirs beyond LEPT_INCLUDE.
    let lang = std::env::var("LANG_MODE").unwrap_or_else(|_| "c".to_string());
    let mut args = if lang == "c++" {
        vec![
            "-x".to_string(),
            "c++".to_string(),
            "-std=c++17".to_string(),
            format!("-I{inc}"),
        ]
    } else {
        vec![
            "-x".to_string(),
            "c".to_string(),
            "-std=c11".to_string(),
            format!("-I{inc}"),
        ]
    };
    if let Ok(extra) = std::env::var("EXTRA_INC") {
        for d in extra.split(':').filter(|d| !d.is_empty()) {
            args.push(format!("-I{d}"));
        }
    }

    let funcs = walk_free_functions(Path::new(&src), &args).map_err(|e| e.to_string())?;
    eprintln!("[harvest] {} free-function definitions in {src}", funcs.len());

    // The intra-TU dispatch graph: for every harvested function, the callees
    // that are ALSO defined in this TU. This is the transcode-driving structure
    // (which functions dispatch to which); a callee with NO in-TU dispatchers of
    // its own is a LEAF — the essential numeric kernel to hand-port. Filtering
    // to the in-TU set drops the libc/leptonica-helper noise so the graph is the
    // dispatch skeleton. General over any C file (scale1.c, enhance.c, …).
    let defined: std::collections::BTreeSet<&str> =
        funcs.iter().map(|f| f.name.as_str()).collect();

    // Optional focus: FAMILY=comma,sep restricts the printed roots (still shows
    // their full in-TU dispatch). Default: every function.
    let family_env = std::env::var("FAMILY").unwrap_or_default();
    let roots: Vec<&str> = if family_env.is_empty() {
        funcs.iter().map(|f| f.name.as_str()).collect()
    } else {
        family_env.split(',').map(str::trim).collect()
    };

    println!("# intra-TU dispatch manifest (ruff_cpp_spo::walk_free_functions on {src})");
    println!("# <function>\tdispatches_to\t<in-TU callees>   ([] = LEAF kernel)");
    for name in roots {
        match funcs.iter().find(|f| f.name == name) {
            Some(f) => {
                let dispatch: Vec<&String> =
                    f.calls.iter().filter(|c| defined.contains(c.as_str())).collect();
                println!(
                    "{}\tdispatches_to\t{:?}\t(+{} non-TU callees)",
                    f.name,
                    dispatch,
                    f.calls.len() - dispatch.len()
                );
            }
            None => println!("{name}\t(not defined in this TU)"),
        }
    }
    Ok(())
}
