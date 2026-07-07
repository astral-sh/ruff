//! Harvest the SHAPES a Rust transcode of Tesseract's Dict/Dawg subsystem
//! needs (D1.2 driver): the `DawgType` / `PermuterType` enums plus the
//! `DawgPosition` / `DawgArgs` struct field lists.
//!
//! Walks `src/dict/dawg.h`, `src/dict/dict.h`, and `src/ccstruct/ratngs.h` via
//! libclang and dumps, per header:
//!
//! - Every ENUM found ([`walk_enums`] for namespace-scope enums, plus the
//!   [`Declaration::Enum`] arm on any class-body enum `walk_tu` surfaces) —
//!   namespace, name, `enum class`-ness, underlying type, and variants.
//! - The `DawgPosition` and `DawgArgs` struct field lists (via [`walk_tu`] —
//!   no new harvester code needed, `Declaration::Field` already covers it).
//!
//! This is a read-only harvest for a hand-rolled Rust shape comparison; it
//! does not drive `model_from_class`/`expand` (no ndjson emission).
//!
//! Run:
//! ```sh
//! TESSERACT_SRC=/tmp/tesseract LIBCLANG_PATH=/usr/lib/llvm-18/lib \
//!   cargo run -p ruff_cpp_spo --features libclang --example harvest_tesseract_dict
//! ```

#![expect(
    clippy::print_stderr,
    clippy::print_stdout,
    reason = "manifest-emission CLI example (mirrors harvest_network)"
)]

use std::path::Path;

use ruff_cpp_spo::{CppEnum, Declaration, walk_enums, walk_tu};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var("TESSERACT_SRC").unwrap_or_else(|_| "/tmp/tesseract".to_string());
    let root = Path::new(&root);
    if !root.join("src/dict/dawg.h").exists() {
        return Err(format!("{} not found; set TESSERACT_SRC", root.display()).into());
    }

    // Tolerate unresolved generated includes (libclang still surfaces the
    // declarations); supply the dict/ccstruct/ccutil/classify include roots,
    // mirroring the ccstruct_motherlode_smoke test's include set.
    let args = [
        "-std=c++17".to_string(),
        "-x".to_string(),
        "c++".to_string(),
        format!("-I{}", root.join("src/dict").display()),
        format!("-I{}", root.join("src/ccstruct").display()),
        format!("-I{}", root.join("src/ccutil").display()),
        format!("-I{}", root.join("src/classify").display()),
        format!("-I{}", root.join("include").display()),
    ];

    let files = [
        "src/dict/dawg.h",
        "src/dict/dict.h",
        "src/ccstruct/ratngs.h",
    ];

    // The struct field lists this driver cares about — everything else from
    // `walk_tu` is printed only incidentally (via the enum arm below).
    let wanted_structs = ["DawgPosition", "DawgArgs"];

    for f in files {
        let path = root.join(f);
        eprintln!("== {f} ==");
        if !path.exists() {
            eprintln!("  (missing, skipping)");
            continue;
        }

        // Namespace-scope enums.
        match walk_enums(&path, &args) {
            Ok(enums) => print_enums(&enums),
            Err(e) => eprintln!("  walk_enums failed: {e}"),
        }

        // Classes (structs) — surfaces both class-body enums (via
        // `Declaration::Enum`) and the wanted struct field lists.
        match walk_tu(&path, &args) {
            Ok(classes) => {
                for c in &classes {
                    let enums: Vec<&CppEnum> = c
                        .declarations
                        .iter()
                        .filter_map(|d| match d {
                            Declaration::Enum(e) => Some(e),
                            _ => None,
                        })
                        .collect();
                    if !enums.is_empty() {
                        print_enums(&enums.into_iter().cloned().collect::<Vec<_>>());
                    }

                    if wanted_structs.contains(&c.name.as_str()) {
                        let qname = c.qualified_name();
                        println!("struct {qname}");
                        for d in &c.declarations {
                            if let Declaration::Field(field) = d {
                                println!("  {}: {}", field.name, field.type_name);
                            }
                        }
                    }
                }
            }
            Err(e) => eprintln!("  walk_tu failed: {e}"),
        }
    }

    Ok(())
}

fn print_enums(enums: &[CppEnum]) {
    for e in enums {
        let qname = if e.namespace.is_empty() {
            e.name.clone()
        } else {
            format!("{}::{}", e.namespace.join("::"), e.name)
        };
        let class_marker = if e.is_class { " class" } else { "" };
        let underlying = if e.underlying_type.is_empty() {
            String::new()
        } else {
            format!(" : {}", e.underlying_type)
        };
        println!("enum{class_marker} {qname}{underlying}");
        for (name, value) in &e.variants {
            println!("  {name} = {value}");
        }
    }
}
