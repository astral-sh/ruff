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
    let args = [
        "-x".to_string(),
        "c".to_string(),
        "-std=c11".to_string(),
        format!("-I{inc}"),
    ];

    let funcs = walk_free_functions(Path::new(&src), &args).map_err(|e| e.to_string())?;
    eprintln!("[harvest] {} free-function definitions in {src}", funcs.len());

    // The pixScale dispatch subtree — the exact functions the byte-exact
    // transcode must follow, in dispatch order. Filter each function's callees
    // to the scale family so the graph is the DISPATCH structure, not every
    // libc/leptonica helper call.
    let family = [
        "pixScale",
        "pixScaleGeneral",
        "pixScaleGrayLI",
        "scaleGrayLILow",
        "pixScaleColorLI",
        "scaleColorLILow",
        "pixScaleAreaMap",
        "pixScaleAreaMap2",
        "pixScaleSmooth",
        "pixScaleBinary",
        "pixUnsharpMasking",
        "pixUnsharpMaskingGray",
        "pixScaleGray2xLI",
        "pixScaleGray4xLI",
    ];
    let is_family = |n: &str| family.contains(&n);

    println!("# pixScale call-graph manifest (ruff_cpp_spo::walk_free_functions on {src})");
    println!("# <function>\tdispatches_to\t<scale-family callees>");
    for name in family {
        match funcs.iter().find(|f| f.name == name) {
            Some(f) => {
                let dispatch: Vec<&String> =
                    f.calls.iter().filter(|c| is_family(c)).collect();
                let all = f.calls.len();
                println!(
                    "{}\tdispatches_to\t{:?}\t(+{} non-family callees)",
                    f.name,
                    dispatch,
                    all - dispatch.len()
                );
            }
            None => println!("{name}\t(not defined in this TU)"),
        }
    }
    Ok(())
}
