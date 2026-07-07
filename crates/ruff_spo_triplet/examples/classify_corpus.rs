//! Run the recipe-centroid classifier over a real harvest ndjson corpus and
//! report the measured recoverable/essential split.
//!
//! Language-agnostic: this reads whatever ndjson a frontend produced (C#,
//! C++, Ruby, Python, …) and classifies every method the harvest saw via
//! `ruff_spo_triplet::{group_functions, classify}`
//! (`.claude/knowledge/fuzzy-recipe-codebook.md` §3). The corpus path is
//! never hardcoded — it always comes from argv or the environment, so no
//! corpus data or output is ever committed.
//!
//! Run:
//! ```sh
//! cargo run -p ruff_spo_triplet --example classify_corpus -- <ndjson-path>
//! # or
//! CORPUS_NDJSON=<ndjson-path> cargo run -p ruff_spo_triplet --example classify_corpus
//! ```

#![expect(
    clippy::print_stdout,
    reason = "the whole point of this example is to print the corpus report"
)]

use std::collections::HashMap;

use ruff_spo_triplet::{RecipeCentroid, classify, from_ndjson, group_functions};

/// Ladder order (`.claude/knowledge/fuzzy-recipe-codebook.md` §3), paired
/// with the label used in the printed histogram.
const CENTROID_ORDER: &[(RecipeCentroid, &str)] = &[
    (RecipeCentroid::Compensate, "Compensate"),
    (RecipeCentroid::Cascade, "Cascade"),
    (RecipeCentroid::Guard, "Guard"),
    (RecipeCentroid::WriteRaise, "WriteRaise"),
    (RecipeCentroid::Default, "Default"),
    (RecipeCentroid::Compute, "Compute"),
    (RecipeCentroid::Normalize, "Normalize"),
    (RecipeCentroid::Observe, "Observe"),
    (RecipeCentroid::Empty, "Empty"),
];

/// Cap on how many essential-tail method names get printed verbatim.
const ESSENTIAL_TAIL_CAP: usize = 40;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("CORPUS_NDJSON").ok())
        .ok_or(
            "usage: classify_corpus <ndjson-path>  (or set CORPUS_NDJSON)\n\
             the corpus path is never hardcoded — pass it explicitly",
        )?;

    let ndjson = std::fs::read_to_string(&path)?;
    let triples = from_ndjson(&ndjson)?;
    let functions = group_functions(&triples);

    let mut counts: HashMap<RecipeCentroid, usize> = HashMap::new();
    let mut essential_tail: Vec<(String, RecipeCentroid)> = Vec::new();
    for f in &functions {
        let centroid = classify(f);
        *counts.entry(centroid).or_insert(0) += 1;
        if matches!(
            centroid,
            RecipeCentroid::Compensate | RecipeCentroid::WriteRaise
        ) {
            essential_tail.push((f.name.clone(), centroid));
        }
    }

    let get = |c: RecipeCentroid| counts.get(&c).copied().unwrap_or(0);
    let total_triaged: usize = functions.len();

    println!("=== recipe centroid histogram ({path}) ===");
    for (centroid, label) in CENTROID_ORDER {
        println!("  {label:<12} {}", get(*centroid));
    }
    println!("  {:<12} {total_triaged}", "TOTAL");

    let recoverable_upper = get(RecipeCentroid::Compute)
        + get(RecipeCentroid::Default)
        + get(RecipeCentroid::Normalize)
        + get(RecipeCentroid::Cascade)
        + get(RecipeCentroid::Guard);
    let essential = get(RecipeCentroid::Compensate) + get(RecipeCentroid::WriteRaise);
    let cascade = get(RecipeCentroid::Cascade);

    let denom_upper = recoverable_upper + essential;
    let denom_lower = denom_upper - cascade;
    let recoverable_lower = recoverable_upper - cascade;

    #[expect(
        clippy::cast_precision_loss,
        reason = "corpus sizes are well within f64's exact-integer range"
    )]
    let pct = |num: usize, denom: usize| -> f64 {
        if denom == 0 {
            0.0
        } else {
            (num as f64 / denom as f64) * 100.0
        }
    };
    let upper_pct = pct(recoverable_upper, denom_upper);
    let lower_pct = pct(recoverable_lower, denom_lower);

    println!();
    println!("total methods triaged (incl. Observe/Empty): {total_triaged}");
    println!(
        "recoverable/essential considered (excl. Observe/Empty): {denom_upper} \
         (recoverable {recoverable_upper} / essential {essential})"
    );
    println!(
        "recoverable% BAND: upper (incl. Cascade) = {upper_pct:.1}% [{recoverable_upper}/{denom_upper}] \
         .. lower (Authoritative-only, Cascade dropped) = {lower_pct:.1}% [{recoverable_lower}/{denom_lower}]"
    );

    println!();
    println!("PRE-REGISTERED BAR: recoverable >= 85% => PASS, < 50% => KILL");
    let verdict = if lower_pct >= 85.0 {
        "PASS (both band ends clear the 85% bar)"
    } else if upper_pct < 50.0 {
        "FAIL / KILL (even the optimistic upper bound is below 50%)"
    } else if upper_pct >= 85.0 {
        "PASS on the upper bound only — lower (Authoritative-only) bound is below 85%; \
         Cascade's Inferred `calls` provenance is load-bearing for this verdict"
    } else {
        "AMBIGUOUS — band straddles the 85%/50% thresholds; neither PASS nor KILL is clean"
    };
    println!("VERDICT: {verdict}");

    println!();
    println!(
        "essential tail (Compensate/WriteRaise — the hand-port worklist), \
         {} total, showing up to {ESSENTIAL_TAIL_CAP}:",
        essential_tail.len()
    );
    for (name, centroid) in essential_tail.iter().take(ESSENTIAL_TAIL_CAP) {
        let label = if matches!(centroid, RecipeCentroid::Compensate) {
            "Compensate"
        } else {
            "WriteRaise"
        };
        println!("  [{label:<10}] {name}");
    }
    if essential_tail.len() > ESSENTIAL_TAIL_CAP {
        println!(
            "  ... and {} more",
            essential_tail.len() - ESSENTIAL_TAIL_CAP
        );
    }

    Ok(())
}
