//! End-to-end exploration over the real `OpenProject` corpus.
//!
//! Runs the AST extractor + expander against `$OPENPROJECT_PATH`, dumps
//! the resulting SPO triples to `/tmp/op_triples.ndjson`, and computes
//! the statistics that let us pick the highest-value next deliverable.
//!
//! Run with:
//!
//! ```text
//! OPENPROJECT_PATH=/home/user/openproject \
//!   cargo test -p ruff_ruby_spo --test op_pipeline_explore \
//!   -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ruff_ruby_spo::extract;
use ruff_spo_triplet::{expand, to_ndjson};

#[test]
#[ignore]
#[allow(clippy::print_stderr, clippy::doc_markdown)] // diagnostic exploration test
fn dump_openproject_triples_and_stats() {
    let Ok(root) = std::env::var("OPENPROJECT_PATH") else {
        eprintln!("OPENPROJECT_PATH unset — skipping");
        return;
    };
    let graph = extract(Path::new(&root));
    let triples = expand(&graph);

    // Write the full ndjson to /tmp so the consumer pipeline can read it.
    let ndjson = to_ndjson(&triples);
    let out = "/tmp/op_triples.ndjson";
    fs::write(out, &ndjson).expect("write ndjson");
    eprintln!("wrote {} bytes to {out}", ndjson.len());

    // Predicate-frequency histogram.
    let mut predicate_freq: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &triples {
        *predicate_freq.entry(t.p.as_str()).or_insert(0) += 1;
    }
    eprintln!("\n=== Predicate frequency ({} triples total) ===", triples.len());
    let mut sorted: Vec<_> = predicate_freq.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    for (p, count) in &sorted {
        eprintln!("  {count:>5}  {p}");
    }

    // Per-model declaration stats (top 20 fattest).
    let mut model_decl_counts: Vec<(&str, usize)> = graph
        .models
        .iter()
        .map(|m| {
            (
                m.name.as_str(),
                m.associations.len()
                    + m.validations.len()
                    + m.callbacks.len()
                    + m.concerns.len()
                    + m.attributes.len()
                    + m.delegations.len()
                    + m.scopes.len()
                    + m.acts_as.len()
                    + m.dsl_calls.len()
                    + m.gem_dsl.len()
                    + m.dynamic_methods.len()
                    + m.refinements.len()
                    + usize::from(m.sti.is_some()),
            )
        })
        .collect();
    model_decl_counts.sort_by_key(|x| std::cmp::Reverse(x.1));
    eprintln!("\n=== Top 20 fattest models ===");
    for (name, count) in model_decl_counts.iter().take(20) {
        eprintln!("  {count:>4}  {name}");
    }

    // Catch-all `has_dsl_call` — what's actually unrouted in real OP corpus?
    let mut unrouted: BTreeMap<&str, usize> = BTreeMap::new();
    for m in &graph.models {
        for dc in &m.dsl_calls {
            *unrouted.entry(dc.name.as_str()).or_insert(0) += 1;
        }
    }
    let mut unrouted_sorted: Vec<_> = unrouted.iter().collect();
    unrouted_sorted.sort_by(|a, b| b.1.cmp(a.1));
    eprintln!("\n=== Catch-all `has_dsl_call` calls (real OP corpus) ===");
    for (name, count) in unrouted_sorted.iter().take(30) {
        eprintln!("  {count:>4}  {name}");
    }
    eprintln!(
        "  ({} distinct names, {} total calls)",
        unrouted.len(),
        unrouted.values().sum::<usize>()
    );

    // Concerns / acts_as / gem_dsl mix — how rich is the AR shape really?
    let assoc: usize = graph.models.iter().map(|m| m.associations.len()).sum();
    let valid: usize = graph.models.iter().map(|m| m.validations.len()).sum();
    let cbk: usize = graph.models.iter().map(|m| m.callbacks.len()).sum();
    let cnc: usize = graph.models.iter().map(|m| m.concerns.len()).sum();
    let attr: usize = graph.models.iter().map(|m| m.attributes.len()).sum();
    let deleg: usize = graph.models.iter().map(|m| m.delegations.len()).sum();
    let sc: usize = graph.models.iter().map(|m| m.scopes.len()).sum();
    let aa: usize = graph.models.iter().map(|m| m.acts_as.len()).sum();
    let gem: usize = graph.models.iter().map(|m| m.gem_dsl.len()).sum();
    let dyn_m: usize = graph.models.iter().map(|m| m.dynamic_methods.len()).sum();
    let dsl: usize = graph.models.iter().map(|m| m.dsl_calls.len()).sum();
    let ref_n: usize = graph.models.iter().map(|m| m.refinements.len()).sum();
    let sti_c: usize = graph.models.iter().filter(|m| m.sti.is_some()).count();
    eprintln!("\n=== Per-category declaration totals ===");
    eprintln!("  associations:      {assoc}");
    eprintln!("  validations:       {valid}");
    eprintln!("  callbacks:         {cbk}");
    eprintln!("  concerns:          {cnc}");
    eprintln!("  attributes:        {attr}");
    eprintln!("  delegations:       {deleg}");
    eprintln!("  scopes:            {sc}");
    eprintln!("  acts_as:           {aa}");
    eprintln!("  dsl_calls:         {dsl}");
    eprintln!("  gem_dsl:           {gem}");
    eprintln!("  dynamic_methods:   {dyn_m}");
    eprintln!("  refinements:       {ref_n}");
    eprintln!("  sti (Option):      {sti_c}");

    // Field / Function side — what's the body-extraction gap?
    let fields: usize = graph.models.iter().map(|m| m.fields.len()).sum();
    let funcs: usize = graph.models.iter().map(|m| m.functions.len()).sum();
    eprintln!("\n=== Body extraction state (D-AR-3.5 stub gap) ===");
    eprintln!("  models:    {}", graph.models.len());
    eprintln!("  fields:    {fields}");
    eprintln!("  functions: {funcs}");

    eprintln!("\nndjson written to {out}");
}
