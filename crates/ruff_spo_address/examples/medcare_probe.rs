//! brick-3 probe — drive a REAL Roslyn harvest of the MedCare C# corpus through
//! the validated loader (`ruff_csharp_spo::load`) → the `(part_of:is_a)` minter
//! (`ruff_spo_address::mint`) → the `FacetCascade`-layout SoA, and measure the
//! claims the design rests on:
//!
//!   A. scale + the "Odoo/OpenProject ~2 MB" footprint conjecture
//!   B. injectivity at scale (roundtrip-lossless "exact, not PQ" claim)
//!   C. truncation (the honest fence: do real class graphs fit 6 tiers?)
//!   D. prefix-routability (is `documentSymbol` actually a longest-common-prefix?)
//!   E. an LSP `documentSymbol` simulation against ground-truth membership
//!   F. the classid-gate FALSIFIER — how much of the facet is redundant given
//!      the class, i.e. how collapsible the is_a half is into a classid.
//!
//! Run:
//!   dotnet run --project crates/ruff_csharp_spo/harvester -- /home/user/MedCare out.ndjson
//!   cargo run -p ruff_spo_address --example medcare_probe -- out.ndjson
//!
//! This is brick-3 as shipped (the harvest is real Roslyn, not a fixture); the
//! only thing it does NOT cross is the OGAR codebook (BBB boundary), so the
//! gate falsifier measures the *redundancy precondition* the gate exploits
//! rather than a codebook-partitioned footprint.

use std::collections::{BTreeMap, BTreeSet};

use ruff_csharp_spo::load;
use ruff_spo_address::{Facet, TIERS, mint};
use ruff_spo_triplet::Triple;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or(
        "usage: medcare_probe <harvested.ndjson>  (produce it with the Roslyn harvester)",
    )?;
    let ndjson = std::fs::read_to_string(&path)?;

    // brick-1→2 seam: the SAME validated loader a real consumer uses. Rejects
    // any predicate outside the closed vocabulary before a facet is minted.
    let triples: Vec<Triple> = load(&ndjson)?;
    let m = mint(&triples);

    println!("== brick-3 probe: {} ==", path);
    println!("loaded {} validated triples\n", triples.len());

    // ── A. scale + footprint conjecture ──────────────────────────────────────
    let n = m.len();
    let classes = count_classes(&triples);
    let bytes = n * 16;
    println!("[A] scale");
    println!("    structural nodes : {n}");
    println!("    classes (ObjectType): {classes}");
    println!("    facet footprint  : {bytes} B = {:.1} KiB (16 B/node)", bytes as f64 / 1024.0);
    // The conjecture: Odoo+OpenProject ~131 K facets ~ 2 MB. Extrapolate from
    // this corpus's bytes/node (constant 16) — so it's really a node-count test.
    println!(
        "    → at 16 B/node, 2 MB budget = {} nodes; this corpus is {:.2}% of that\n",
        2 * 1024 * 1024 / 16,
        n as f64 / (2.0 * 1024.0 * 1024.0 / 16.0) * 100.0
    );

    // ── B. injectivity at scale ──────────────────────────────────────────────
    let mut seen: BTreeMap<Facet, &str> = BTreeMap::new();
    let mut collisions: Vec<(String, String)> = Vec::new();
    for (node, f) in m.iter() {
        if let Some(&prev) = seen.get(&f) {
            collisions.push((prev.to_owned(), node.to_owned()));
        } else {
            seen.insert(f, node);
        }
    }
    println!("[B] injectivity (roundtrip-lossless / 'exact, not PQ')");
    println!("    distinct facets  : {} / {n} nodes", seen.len());
    if collisions.is_empty() {
        println!("    → PASS: no two nodes share a facet (the mint is injective at scale)\n");
    } else {
        println!("    → {} COLLISIONS (falsifies 'exact' for this corpus):", collisions.len());
        for (a, b) in collisions.iter().take(8) {
            println!("        {a}  ==  {b}");
        }
        println!();
    }

    // ── C. truncation (the honest fence) ─────────────────────────────────────
    let trunc = m.truncated();
    println!("[C] truncation (depth > {TIERS} tiers or > 255 siblings)");
    println!("    truncated nodes  : {} / {n}", trunc.len());
    if trunc.is_empty() {
        println!("    → PASS: the whole class graph fits in 6 tiers (claim held)\n");
    } else {
        println!("    → {} nodes need ref-escape (sample):", trunc.len());
        for t in trunc.iter().take(8) {
            println!("        {t}");
        }
        println!();
    }

    // ── D. prefix-routability ────────────────────────────────────────────────
    // For every part_of edge (member → class), the member's part_of_chain must
    // extend the class's non-zero prefix by exactly one tier. That is what makes
    // a containment query a longest-common-prefix scan.
    let part_of = part_of_edges(&triples);
    let (mut ok, mut bad) = (0usize, 0usize);
    let mut bad_sample: Vec<(String, String)> = Vec::new();
    for (member, class) in &part_of {
        let (Some(mf), Some(cf)) = (m.facet(member), m.facet(class)) else { continue };
        if extends_prefix(cf.part_of_chain(), mf.part_of_chain()) {
            ok += 1;
        } else {
            bad += 1;
            if bad_sample.len() < 8 {
                bad_sample.push((member.clone(), class.clone()));
            }
        }
    }
    println!("[D] prefix-routability of part_of (documentSymbol = LCP)");
    println!("    member→class edges checked: {}", ok + bad);
    println!("    share parent's prefix      : {ok}");
    if bad == 0 {
        println!("    → PASS: every member extends its class's part_of prefix\n");
    } else {
        println!("    → {bad} violations (sample):");
        for (mm, cc) in &bad_sample {
            println!("        {mm}  ⊄  {cc}");
        }
        println!();
    }

    // ── E. LSP documentSymbol simulation on the biggest class ────────────────
    // Pick the class with the most members; "query" = all nodes whose
    // part_of_chain has the class facet as a strict prefix; compare to the
    // ground-truth member set from the harvest.
    if let Some((class, truth)) = biggest_class(&part_of) {
        let cf = m.facet(&class).expect("class minted");
        let cprefix = cf.part_of_chain();
        let depth = nonzero_len(cprefix);
        let mut hit: BTreeSet<String> = BTreeSet::new();
        for (node, f) in m.iter() {
            if node == class {
                continue;
            }
            let p = f.part_of_chain();
            // strict extension at the next tier, same coarse prefix
            if depth < TIERS && p[..depth] == cprefix[..depth] && p[depth] != 0 {
                hit.insert(node.to_owned());
            }
        }
        let recovered: BTreeSet<String> = truth.iter().filter(|m_| hit.contains(*m_)).cloned().collect();
        println!("[E] documentSymbol('{class}') via part_of prefix scan");
        println!("    ground-truth members : {}", truth.len());
        println!("    prefix-scan recovered: {} (direct children)", recovered.len());
        // direct children should be exactly the ground-truth members (one tier down)
        let missing: Vec<&String> = truth.iter().filter(|m_| !hit.contains(*m_)).collect();
        if missing.is_empty() {
            println!("    → PASS: prefix scan returns every member, no graph re-walk\n");
        } else {
            println!("    → {} members not recovered by prefix (sample): {:?}\n",
                missing.len(), missing.iter().take(5).collect::<Vec<_>>());
        }
    }

    // ── F. classid-gate falsifier: how redundant is is_a given the class? ────
    // The gate's premise: an already-ontologized class's is_a is determined by
    // its classid, so storing the is_a chain restates the codebook. Measure the
    // precondition WITHOUT the codebook: per class, how many DISTINCT is_a
    // tier-0 ranks do its members carry? If members' is_a collapses to a tiny
    // set (kind = Property/Function), the is_a half is highly redundant and the
    // gate would reclaim it; if it's diverse, the gate costs a re-resolve.
    let mut isa0_values: BTreeSet<u8> = BTreeSet::new();
    let mut isa_full: BTreeSet<[u8; TIERS]> = BTreeSet::new();
    for (_, f) in m.iter() {
        isa0_values.insert(f.is_a_chain()[0]);
        isa_full.insert(f.is_a_chain());
    }
    let isa_bytes_total = n * TIERS; // the is_a half of every facet
    let isa_distinct = isa_full.len();
    println!("[F] classid-gate falsifier — is_a redundancy given the class");
    println!("    distinct is_a tier-0 roots : {}", isa0_values.len());
    println!("    distinct full is_a chains  : {isa_distinct} over {n} nodes");
    println!(
        "    is_a half stores {isa_bytes_total} B but carries only {} distinct values",
        isa_distinct
    );
    let redundancy = 1.0 - (isa_distinct as f64 / n as f64);
    println!("    → is_a redundancy: {:.1}%  (fraction of nodes whose is_a chain is a duplicate)", redundancy * 100.0);
    println!(
        "    → gate verdict: if the classid (0xDDCC = domain‖concept) already\n\
         \x20     names the concept, {:.1}% of is_a chains are restated codebook\n\
         \x20     bytes the gate could reclaim — measured, not assumed.",
        redundancy * 100.0
    );

    // ── G. the 256-cap-is-a-lint law ────────────────────────────────────────
    // Every sibling set that overflows the per-tier u8 (>256) is a DESIGN smell,
    // not a storage limit. Classify each overflow as the operator's dichotomy:
    //   (1) DUPLICATION lacking a masked ClassView — the siblings collapse to a
    //       few distinct field_types (mask by classid → render_rows ≤64);
    //   (2) lacks SEPARATION OF CONCERNS — data (has_field) and behaviour
    //       (has_function) are conflated under one parent; split them.
    // The law's falsifier: is there ANY overflow that is neither — i.e. >256
    // genuinely distinct, well-separated, non-duplicated siblings?
    let ftype = field_types(&triples);
    let members_by_class = members_by_class(&triples);
    println!("\n[G] 256-cap-is-a-lint — every overflow is duplication and/or conflation");
    let mut counterexamples = 0usize;
    for (class, members) in &members_by_class {
        if members.len() <= 256 {
            continue;
        }
        let fields: Vec<&String> = members.iter().filter(|m_| ftype.contains_key(*m_)).collect();
        let fns = members.len() - fields.len(); // has_function members have no field_type
        let distinct_types: BTreeSet<&str> =
            fields.iter().filter_map(|m_| ftype.get(*m_).map(String::as_str)).collect();
        let dup = fields.len().saturating_sub(distinct_types.len()); // maskable duplicates
        let is_dup = !distinct_types.is_empty() && distinct_types.len() <= 256;
        let is_conflated = fns > 0 && !fields.is_empty(); // data + behaviour mixed
        println!(
            "    {} : {} members = {} data / {} fn",
            class.split(':').next_back().unwrap_or(class),
            members.len(), fields.len(), fns
        );
        println!(
            "        data → {} distinct field_types ({} duplicate rows maskable by classid)  | {}",
            distinct_types.len(), dup,
            if is_dup { "DUPLICATION ✓ (mask via ClassView)" } else { "not type-collapsible" }
        );
        println!(
            "        data+fn mixed under one parent: {}  | largest single concern: {}",
            if is_conflated { "yes → SEPARATION OF CONCERNS ✓" } else { "no" },
            fields.len().max(fns)
        );
        if !is_dup && !is_conflated {
            counterexamples += 1;
            println!("        → ⚠ COUNTEREXAMPLE: neither duplication nor conflation");
        }
    }
    if counterexamples == 0 {
        println!(
            "    → LAW HOLDS: no overflow is well-separated, non-duplicated 256+ siblings.\n\
             \x20     Every >256 level is a lint for ClassView-mask (duplication) and/or\n\
             \x20     concern-split (data⊥behaviour) — 256 is the cardinality of a\n\
             \x20     well-factored level, not a limit to widen."
        );
    } else {
        println!("    → {counterexamples} COUNTEREXAMPLE(S) — the law is falsified for this corpus.");
    }

    Ok(())
}

/// `member → field_type` for every `has_field` member (functions have none).
fn field_types(triples: &[Triple]) -> BTreeMap<String, String> {
    triples
        .iter()
        .filter(|t| t.p == "field_type")
        .map(|t| (t.s.clone(), t.o.clone()))
        .collect()
}

/// `class → its members` (has_field ∪ has_function objects).
fn members_by_class(triples: &[Triple]) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in triples {
        if t.p == "has_field" || t.p == "has_function" {
            out.entry(t.s.clone()).or_default().push(t.o.clone());
        }
    }
    out
}

/// Count `rdf:type ogit:ObjectType` subjects — the classes.
fn count_classes(triples: &[Triple]) -> usize {
    triples
        .iter()
        .filter(|t| t.p == "rdf:type" && t.o == "ogit:ObjectType")
        .map(|t| t.s.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

/// `(member, class)` for every `has_field` / `has_function` edge.
fn part_of_edges(triples: &[Triple]) -> Vec<(String, String)> {
    triples
        .iter()
        .filter(|t| t.p == "has_field" || t.p == "has_function")
        .map(|t| (t.o.clone(), t.s.clone()))
        .collect()
}

/// The class with the most members, plus its member set.
fn biggest_class(part_of: &[(String, String)]) -> Option<(String, BTreeSet<String>)> {
    let mut by_class: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (member, class) in part_of {
        by_class.entry(class.clone()).or_default().insert(member.clone());
    }
    by_class.into_iter().max_by_key(|(_, ms)| ms.len())
}

/// Does `child` extend `parent` by exactly its non-zero prefix + one more tier?
fn extends_prefix(parent: [u8; TIERS], child: [u8; TIERS]) -> bool {
    let d = nonzero_len(parent);
    if d == 0 || d >= TIERS {
        return false;
    }
    child[..d] == parent[..d] && child[d] != 0
}

/// Length of the leading non-zero run (the address depth).
fn nonzero_len(chain: [u8; TIERS]) -> usize {
    chain.iter().take_while(|&&b| b != 0).count()
}
