//! Harvest the OpenProject AR-shape manifest from a real Rails source tree.
//!
//! Runs `ruff_ruby_spo::extract(<source>)` + `ruff_spo_triplet::expand` and
//! writes the full ndjson triple stream to `MANIFEST_OUT` (default
//! `/tmp/op_triples.ndjson`). This is the canonical refresh path for the
//! downstream `op-surreal-ast::triples_to_schema` consumer in
//! [openproject-nexgen-rs](https://github.com/AdaWorldAPI/openproject-nexgen-rs)
//! — that crate's `op_schema_explore` integration test reads the same file
//! to verify the AR-shape lowering on real data.
//!
//! Run:
//! ```sh
//! OP_SRC=/path/to/openproject cargo run -p ruff_ruby_spo --example harvest_op
//! # writes /tmp/op_triples.ndjson
//! ```
//!
//! Defaults:
//! - `OP_SRC` → `/home/user/openproject` (where AdaWorldAPI's worktree lives
//!   on the dev image; mirrors the existing examples in this crate family).
//! - `MANIFEST_OUT` → `/tmp/op_triples.ndjson`.
//!
//! The OpenProject corpus stays UPSTREAM and is never vendored (iron rule
//! of the harvester family — see `ruff_cpp_spo/examples/harvest_unicharset.rs`).

#![expect(
    clippy::print_stderr,
    reason = "manifest-emission CLI example (mirrors the cpp_spo harvester)"
)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use ruff_ruby_spo::extract;
use ruff_spo_triplet::{expand, to_ndjson};

fn main() {
    let src = env::var("OP_SRC").unwrap_or_else(|_| "/home/user/openproject".to_string());
    let out = env::var("MANIFEST_OUT").unwrap_or_else(|_| "/tmp/op_triples.ndjson".to_string());

    let src_path = PathBuf::from(&src);
    if !src_path.exists() {
        eprintln!(
            "error: OP source path `{src}` does not exist.\n\
             Set OP_SRC to the OpenProject Rails source root \
             (the directory containing `app/`)."
        );
        process::exit(2);
    }

    eprintln!("harvesting OP AR-shape from {src} ...");
    let graph = extract(&src_path);
    eprintln!("  models found: {}", graph.models.len());

    let triples = expand(&graph);
    eprintln!("  triples emitted: {}", triples.len());

    let ndjson = to_ndjson(&triples);
    let out_path = PathBuf::from(&out);
    if let Err(err) = fs::write(&out_path, &ndjson) {
        eprintln!("error writing {out}: {err}");
        process::exit(1);
    }
    eprintln!(
        "wrote {} bytes ({} triples) to {out}",
        ndjson.len(),
        triples.len(),
    );
}
