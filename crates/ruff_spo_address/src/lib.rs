//! `ruff_spo_address` — the deterministic `(part_of:is_a)` rank-minter.
//!
//! This is the one genuinely-new brick between the `ruff_*_spo` SPO harvest and
//! the lance-graph `(part_of:is_a)` GUID SoA (see lance-graph
//! `.claude/knowledge/ast-as-partof-isa-address.md` — "The missing brick"). The
//! carrier ([`lance_graph_contract::facet::FacetCascade`], shipped #613/#614) is
//! already there; this crate fills the *mint*.
//!
//! Given a corpus's two structural relations:
//!
//! - **part_of** (mereology / membership) — harvested as `has_field` /
//!   `has_function` (`class → member`, so the member is *part_of* the class);
//! - **is_a** (taxonomy / typing) — harvested as `inherits_from` (`class →
//!   base`) and, for leaves, `rdf:type` (`member → kind`);
//!
//! it assigns every node a deterministic `(part_of_rank, is_a_rank)` at each of
//! the **6 cascade tiers** and packs them into a 16-byte [`Facet`] whose bytes
//! are **layout-identical** to `FacetCascade`:
//!
//! ```text
//! facet (16 B) = facet_classid(4 LE) | 6 × (lo:hi)
//!   byte[0..4)        = facet_classid (LE)
//!   byte[4 + 2·t]     = is_a_rank[t]   (the FacetCascade `lo` / lo_chain)
//!   byte[5 + 2·t]     = part_of_rank[t](the FacetCascade `hi` / hi_chain)
//! ```
//!
//! So [`Facet::part_of_chain`] == `FacetCascade::hi_chain` and
//! [`Facet::is_a_chain`] == `FacetCascade::lo_chain`. Both chains are
//! **prefix-routable**: two nodes in the same part_of subtree share a leading
//! `part_of_chain` prefix (a `documentSymbol` / containment query is a longest-
//! common-prefix), and two nodes under the same supertype share a leading
//! `is_a_chain` prefix (a `typeHierarchy` walk).
//!
//! # Exact, not a PQ approximation — but only below the per-tier cap
//!
//! The ranks are a deterministic assignment (sorted sibling order), not learned
//! centroids, so within the address budget the mint is roundtrip-lossless and
//! iron-rule clean per `I-VSA-IDENTITIES` (it encodes *identity positions* —
//! which class, which base, which slot — never bundles content).
//!
//! **The budget is real and measured, not just asymptotic.** Each tier is one
//! `u8`, so a node with more than **255 siblings** at some tier saturates to
//! rank 255 and is flagged in [`Mint::truncated`]; once two nodes share a
//! saturated rank at every tier their facets collide. Verified against a real
//! multi-thousand-node corpus, the failure has two distinct causes: **God-
//! classes** (a single class with hundreds of fields, e.g. a large UI form)
//! overflowing the part_of axis, and **flat is_a roots** (a kind-discriminator
//! type with thousands of direct children, e.g. every "Property" or "Function"
//! node parented straight under one root) overflowing the is_a axis. So
//! "exact" holds for a class graph whose every sibling set is ≤ 255 and depth
//! ≤ 6 — NOT for arbitrary real corpora. The flat-is_a-root case is the
//! dominant one and is addressable: a member's *kind* (e.g. Property/Function)
//! belongs in its `facet_classid`, not in a 6-tier sibling rank under a
//! mega-root. [`mint_factored`] is the corrected minter that fixes both
//! failure modes (base-255 positional part_of paths + is_a built from
//! `inherits_from` only, kind moved to a bounded leaf enum).
//!
//! # `facet_classid`
//!
//! [`mint`] leaves `facet_classid = 0` (the bare `(part_of:is_a)` address).
//! [`mint_with_classid`] takes a resolver so a caller holding the OGAR codebook
//! can stamp the canonical class-id BBB-safely — e.g.
//! `|iri| lance_graph_contract::canonical_concept_id(concept_of(iri)).map_or(0, u32::from)`
//! — without this crate ever depending on the codebook (pure std + the SPO
//! triplet vocab only).

pub mod soc;

use ruff_spo_triplet::Triple;
use std::collections::{BTreeMap, BTreeSet};

/// The number of `(part_of:is_a)` cascade tiers in a facet (the `FacetCascade`
/// 6-tier address: `HEEL·HIP·TWIG·LEAF·family·identity`).
pub const TIERS: usize = 6;

/// A 16-byte `(part_of:is_a)` facet, byte-identical to
/// `lance_graph_contract::facet::FacetCascade`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Facet {
    bytes: [u8; 16],
}

impl Facet {
    /// Pack a facet from its three components. `part_of` / `is_a` are the
    /// coarse→fine rank chains (`part_of[t]` = the `hi` byte of tier `t`,
    /// `is_a[t]` = the `lo` byte) — matching the `FacetCascade` wire order
    /// (`byte[4+2t] = lo`, `byte[5+2t] = hi`).
    #[must_use]
    pub const fn from_parts(facet_classid: u32, part_of: [u8; TIERS], is_a: [u8; TIERS]) -> Self {
        let c = facet_classid.to_le_bytes();
        let mut b = [0u8; 16];
        b[0] = c[0];
        b[1] = c[1];
        b[2] = c[2];
        b[3] = c[3];
        let mut t = 0;
        while t < TIERS {
            b[4 + 2 * t] = is_a[t]; // lo
            b[5 + 2 * t] = part_of[t]; // hi
            t += 1;
        }
        Self { bytes: b }
    }

    /// The 16 facet bytes — feed straight to `FacetCascade::from_bytes`.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 16] {
        self.bytes
    }

    /// The class-id stamped into bytes `[0..4)` (`0` from [`mint`]).
    #[must_use]
    pub const fn facet_classid(self) -> u32 {
        u32::from_le_bytes([self.bytes[0], self.bytes[1], self.bytes[2], self.bytes[3]])
    }

    /// The **part_of** chain (coarse→fine) — the `FacetCascade` `hi_chain`.
    #[must_use]
    pub const fn part_of_chain(self) -> [u8; TIERS] {
        let b = &self.bytes;
        [b[5], b[7], b[9], b[11], b[13], b[15]]
    }

    /// The **is_a** chain (coarse→fine) — the `FacetCascade` `lo_chain`.
    #[must_use]
    pub const fn is_a_chain(self) -> [u8; TIERS] {
        let b = &self.bytes;
        [b[4], b[6], b[8], b[10], b[12], b[14]]
    }
}

/// The result of minting a corpus: each node's [`Facet`], plus the nodes whose
/// part_of/is_a depth exceeded the 6 tiers (or whose sibling count exceeded the
/// 255-per-tier byte) and were therefore truncated.
#[derive(Clone, Debug, Default)]
pub struct Mint {
    facets: BTreeMap<String, Facet>,
    truncated: Vec<String>,
}

impl Mint {
    /// The facet minted for `node`, if it is a structural node in the corpus.
    #[must_use]
    pub fn facet(&self, node: &str) -> Option<Facet> {
        self.facets.get(node).copied()
    }

    /// Number of minted nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.facets.len()
    }

    /// Whether nothing was minted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.facets.is_empty()
    }

    /// Iterate `(node, facet)` in deterministic node order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Facet)> {
        self.facets.iter().map(|(k, &v)| (k.as_str(), v))
    }

    /// Nodes whose address was truncated (part_of/is_a depth > 6 tiers, or a
    /// sibling set larger than 255). Empty for a corpus that fits — the honest
    /// fence on "exact": beyond the cap the facet is a routing prefix, not a
    /// lossless address (deeper levels are the registry/ref-escape's job).
    #[must_use]
    pub fn truncated(&self) -> &[String] {
        &self.truncated
    }

    /// Reverse lookup: the node a facet addresses, if any. `O(n)`; intended for
    /// tests / injectivity checks, not a hot path.
    #[must_use]
    pub fn node_of(&self, facet: Facet) -> Option<&str> {
        self.facets
            .iter()
            .find(|&(_, &f)| f == facet)
            .map(|(k, _)| k.as_str())
    }
}

/// Which chain a [`RadixCodebook`] is ordered by — the two prefix-routable axes
/// of a [`Facet`] (`5+2t` bytes = part_of / hi; `4+2t` bytes = is_a / lo).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    /// part_of (mereology / containment) — the `documentSymbol` axis (`hi_chain`).
    PartOf,
    /// is_a (taxonomy / inheritance) — the `typeHierarchy` axis (`lo_chain`).
    IsA,
}

/// A cheap, **on-demand** radix-trie codebook over one [`Axis`] of a [`Mint`].
///
/// "Radix trie" without the pointers: the 6-byte chains are sorted, so nodes
/// sharing an ancestor are *contiguous*, and a prefix query is a binary-search
/// range over cache-resident bytes — no trie-node allocation, no SPO re-walk.
/// This is the materialized form of the `(part_of:is_a)` address kept from the
/// `4+2t` / `5+2t` packing, built only when a prefix-query workload needs it.
///
/// # When to build it (PROS) — modest-cardinality, readable, prefix-query work
///
/// The Odoo / Redmine / MedCare *app-concept* scale (hundreds–thousands of nodes):
///
/// - **O(log n + k) prefix queries.** `documentSymbol(class)` /
///   `typeHierarchy(base)` become a range scan; no graph traversal.
/// - **Cache-resident, zero standing cost.** The sorted chain bytes *are* the
///   radix order; built from facets you already hold, dropped when the workload
///   ends. Nothing is stored in the layout — the gate is "call the method", not
///   "feature-flag the address".
/// - **Readable.** Each key is the legible part_of/is_a address, not a hash — you
///   route, group, and reason on it without decoding a value.
///
/// # When NOT to build it (CONS)
///
/// - **Derived view.** Rebuild (O(n log n)) if the mint changes; it is a query
///   accelerator, not a store.
/// - **Memory ∝ nodes.** For cardinalities you would never enumerate (genome
///   scale — ~3.2 B alleles), do **not** materialize a codebook: address
///   *positionally* (the chain IS the coordinate — compute, don't look up). The
///   codebook is for the regime where a table is affordable AND the structure is
///   worth keeping readable; past that it is the wrong tool.
/// - **One axis per codebook.** part_of and is_a sort differently; build both
///   only if you need both query directions.
pub struct RadixCodebook {
    axis: Axis,
    /// `(chain, node)` sorted by `chain` — the implicit radix order.
    entries: Vec<([u8; TIERS], String)>,
}

impl RadixCodebook {
    /// The axis this codebook is ordered by.
    #[must_use]
    pub fn axis(&self) -> Axis {
        self.axis
    }

    /// Number of `(chain, node)` entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the codebook is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every `(chain, node)` whose chain shares `prefix`'s first `depth` tiers —
    /// the contiguous range under that ancestor. `documentSymbol` (PartOf) /
    /// `typeHierarchy` (IsA) in `O(log n + k)`, no graph walk. `depth = 0`
    /// returns the whole codebook.
    #[must_use]
    pub fn under_prefix(&self, prefix: [u8; TIERS], depth: usize) -> &[([u8; TIERS], String)] {
        let d = depth.min(TIERS);
        let lo = self.entries.partition_point(|(c, _)| c[..d] < prefix[..d]);
        let hi = lo + self.entries[lo..].partition_point(|(c, _)| c[..d] == prefix[..d]);
        &self.entries[lo..hi]
    }
}

impl Mint {
    /// Build a cheap radix-trie codebook over one [`Axis`], **on demand**. The
    /// gate for "store the trie vs resolve it" is this method call — see
    /// [`RadixCodebook`] for the PROS/CONS that decide whether you want it for a
    /// given workload (yes for app-concept scale; no for genome-scale — address
    /// positionally there).
    #[must_use]
    pub fn radix_codebook(&self, axis: Axis) -> RadixCodebook {
        let mut entries: Vec<([u8; TIERS], String)> = self
            .facets
            .iter()
            .map(|(n, f)| {
                let chain = match axis {
                    Axis::PartOf => f.part_of_chain(),
                    Axis::IsA => f.is_a_chain(),
                };
                (chain, n.clone())
            })
            .collect();
        entries.sort_unstable();
        RadixCodebook { axis, entries }
    }
}

/// Mint `(part_of:is_a)` facets with `facet_classid = 0` (the bare address).
#[must_use]
pub fn mint(triples: &[Triple]) -> Mint {
    mint_with_classid(triples, |_| 0)
}

/// Mint `(part_of:is_a)` facets, stamping each node's `facet_classid` via
/// `classid_of` (e.g. an OGAR codebook resolver — see crate docs).
#[must_use]
pub fn mint_with_classid(triples: &[Triple], classid_of: impl Fn(&str) -> u32) -> Mint {
    // ── 1. Build the two forests from the structural predicates. ──
    // part_of: member → its container class (has_field / has_function inverted).
    let mut po_parent: BTreeMap<&str, &str> = BTreeMap::new();
    // is_a: class → base (inherits_from) takes priority; else node → kind (rdf:type).
    let mut ia_inherit: BTreeMap<&str, &str> = BTreeMap::new();
    let mut ia_type: BTreeMap<&str, &str> = BTreeMap::new();
    // The set of real graph nodes (subjects + structural objects; never literals).
    let mut nodes: BTreeSet<&str> = BTreeSet::new();

    for t in triples {
        let (s, p, o) = (t.s.as_str(), t.p.as_str(), t.o.as_str());
        match p {
            "has_field" | "has_function" => {
                po_parent.insert(o, s); // o is part_of s
                nodes.insert(s);
                nodes.insert(o);
            }
            "inherits_from" => {
                ia_inherit.insert(s, o);
                nodes.insert(s);
                nodes.insert(o);
            }
            "rdf:type" => {
                ia_type.insert(s, o);
                nodes.insert(s);
                nodes.insert(o);
            }
            // field_type / is_static / … carry literal objects — subject is a
            // node, object is not.
            _ => {
                nodes.insert(s);
            }
        }
    }

    // is_a parent = inherits_from if present, else rdf:type.
    let ia_parent: BTreeMap<&str, &str> = nodes
        .iter()
        .filter_map(|&n| {
            ia_inherit
                .get(n)
                .or_else(|| ia_type.get(n))
                .map(|&p| (n, p))
        })
        .collect();

    // ── 2. Children (sorted) + roots, for each forest — the rank basis. ──
    let (po_children, po_roots) = forest(&nodes, &po_parent);
    let (ia_children, ia_roots) = forest(&nodes, &ia_parent);

    // ── 3. Mint each node. ──
    let mut out = Mint::default();
    for &n in &nodes {
        let (po, po_trunc) = ranks(n, &po_parent, &po_children, &po_roots);
        let (ia, ia_trunc) = ranks(n, &ia_parent, &ia_children, &ia_roots);
        out.facets
            .insert(n.to_owned(), Facet::from_parts(classid_of(n), po, ia));
        if po_trunc || ia_trunc {
            out.truncated.push(n.to_owned());
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────
// Factored mint — the 256-cap-is-a-lint law turned into a correct address.
// ─────────────────────────────────────────────────────────────────────────

/// Mint injective `(part_of:is_a)` facets that respect the per-tier 255 cap by
/// **cascading deeper instead of saturating** (the OGAR canon: "scale = the
/// next cascade level, never field-widening").
///
/// The naive [`mint`] ranks each sibling set into one `u8` tier, so a parent
/// with > 255 children (a God-class, or an `rdf:type` kind mega-root)
/// saturates and collides (see the crate-level docs' "budget is real and
/// measured" section). This minter fixes both failure modes the 256-cap law
/// names:
///
/// - **duplication** (a kind-discriminator mega-root, e.g. every `Property` /
///   `Function` node parented straight under one root): is_a is built from
///   `inherits_from` **only** (the real inheritance fan-out stays far below
///   255, so it never explodes); a member's *kind* becomes a bounded leaf enum
///   (`field` = 1, `fn` = 2), not a ranked child of a many-thousand-wide root.
/// - **conflation** (a class with > 255 members): the part_of address is a
///   **base-255 positional path** — each generation consumes
///   `ceil(log255(sibling_count))` tiers, so no tier exceeds 255 and the address
///   stays injective and prefix-routable (a child's chain extends its parent's).
///
/// Digits are `1..=255`; `0` stays reserved as the "unused tier" sentinel, so
/// [`Facet::part_of_chain`]'s leading-non-zero prefix still means "address
/// depth." Truncation (a node whose path needs > 6 tiers) is still reported in
/// [`Mint::truncated`], but for class graphs it does not occur (root classes →
/// 2 tiers, members → 2 tiers, total ≤ 4).
#[must_use]
pub fn mint_factored(triples: &[Triple]) -> Mint {
    // ── 1. part_of forest (member → owning class) + is_a from inherits ONLY. ──
    let mut po_parent: BTreeMap<&str, &str> = BTreeMap::new();
    let mut ia_inherit: BTreeMap<&str, &str> = BTreeMap::new();
    let mut kind: BTreeMap<&str, u8> = BTreeMap::new(); // 1=field/Property 2=fn/Function 3=ObjectType
    let mut nodes: BTreeSet<&str> = BTreeSet::new();

    for t in triples {
        let (s, p, o) = (t.s.as_str(), t.p.as_str(), t.o.as_str());
        match p {
            "has_field" => {
                po_parent.insert(o, s);
                nodes.insert(s);
                nodes.insert(o);
                kind.entry(o).or_insert(1);
            }
            "has_function" => {
                po_parent.insert(o, s);
                nodes.insert(s);
                nodes.insert(o);
                kind.entry(o).or_insert(2);
            }
            "inherits_from" => {
                ia_inherit.insert(s, o);
                nodes.insert(s);
                nodes.insert(o);
            }
            "rdf:type" => {
                let k = match o {
                    "ogit:Property" => 1,
                    "ogit:Function" => 2,
                    _ => 3,
                };
                kind.insert(s, k);
                nodes.insert(s);
            }
            _ => {
                nodes.insert(s);
            }
        }
    }

    let (po_children, po_roots) = forest(&nodes, &po_parent);
    let (ia_children, ia_roots) = forest(&nodes, &ia_inherit);

    // ── 2. part_of: base-255 positional spread from the virtual root. ──
    let mut part_of: BTreeMap<&str, [u8; TIERS]> = BTreeMap::new();
    let mut truncated: BTreeSet<&str> = BTreeSet::new();
    assign_b255(&po_roots, &po_children, [0; TIERS], 0, &mut part_of, &mut truncated);

    // ── 3. is_a: inherits lineage for classes; bounded kind enum for members. ──
    let mut is_a: BTreeMap<&str, [u8; TIERS]> = BTreeMap::new();
    assign_b255(&ia_roots, &ia_children, [0; TIERS], 0, &mut is_a, &mut truncated);
    // Members (no inherits edge) carry their kind in is_a tier-0 instead of an
    // inherits-tree position — the masked, never-exploding kind discriminator.
    for &n in &nodes {
        if !ia_inherit.contains_key(n) && !ia_children.contains_key(n) {
            let mut chain = [0u8; TIERS];
            chain[0] = *kind.get(n).unwrap_or(&0);
            is_a.insert(n, chain);
        }
    }

    // ── 4. pack. ──
    let mut out = Mint::default();
    for &n in &nodes {
        let po = part_of.get(n).copied().unwrap_or([0; TIERS]);
        let ia = is_a.get(n).copied().unwrap_or([0; TIERS]);
        out.facets.insert(n.to_owned(), Facet::from_parts(0, po, ia));
    }
    out.truncated = truncated.into_iter().map(str::to_owned).collect();
    out
}

/// Smallest number of base-255 digits (each `1..=255`) needed to index a sibling
/// set of `n` members (`0` is reserved as the unused-tier sentinel, so one tier
/// holds 255 distinct siblings, not 256).
const fn b255_width(n: usize) -> usize {
    let mut w = 1;
    let mut cap = 255usize;
    while cap < n {
        cap = cap.saturating_mul(255);
        w += 1;
    }
    w
}

/// Write the base-255 digits of 0-based sibling index `idx` into
/// `out[at..at+w]`, big-endian, each digit `1..=255`.
fn b255_write(idx: usize, w: usize, out: &mut [u8; TIERS], at: usize) {
    let mut x = idx;
    for k in (0..w).rev() {
        if at + k < TIERS {
            out[at + k] = ((x % 255) as u8) + 1;
        }
        x /= 255;
    }
}

/// Assign a base-255 positional chain to every node in a forest: each node's
/// chain extends its parent's by `ceil(log255(sibling_count))` digit-tiers.
/// Nodes whose path would exceed [`TIERS`] are added to `truncated`.
fn assign_b255<'a>(
    siblings: &[&'a str],
    children: &BTreeMap<&'a str, Vec<&'a str>>,
    parent_chain: [u8; TIERS],
    tier: usize,
    out: &mut BTreeMap<&'a str, [u8; TIERS]>,
    truncated: &mut BTreeSet<&'a str>,
) {
    if siblings.is_empty() {
        return;
    }
    let w = b255_width(siblings.len());
    for (i, &node) in siblings.iter().enumerate() {
        let mut chain = parent_chain;
        if tier + w > TIERS {
            truncated.insert(node);
        } else {
            b255_write(i, w, &mut chain, tier);
        }
        out.insert(node, chain);
        if let Some(kids) = children.get(node) {
            assign_b255(kids, children, chain, tier + w, out, truncated);
        }
    }
}

/// Build `(parent → sorted children, sorted roots)` for one forest.
fn forest<'a>(
    nodes: &BTreeSet<&'a str>,
    parent: &BTreeMap<&'a str, &'a str>,
) -> (BTreeMap<&'a str, Vec<&'a str>>, Vec<&'a str>) {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut roots: Vec<&str> = Vec::new();
    for &n in nodes {
        match parent.get(n) {
            Some(&p) => children.entry(p).or_default().push(n),
            None => roots.push(n),
        }
    }
    // BTreeSet iteration is already sorted, so `children` / `roots` are sorted;
    // make it explicit so the determinism contract is local, not incidental.
    for v in children.values_mut() {
        v.sort_unstable();
    }
    roots.sort_unstable();
    (children, roots)
}

/// The coarse→fine rank chain for `node` in one forest. Rank at tier `t` is the
/// 1-based index of the ancestor at depth `t` among its siblings (1..=255; 0
/// means "tier below this node's depth"). Returns `(chain, truncated)`.
fn ranks(
    node: &str,
    parent: &BTreeMap<&str, &str>,
    children: &BTreeMap<&str, Vec<&str>>,
    roots: &[&str],
) -> ([u8; TIERS], bool) {
    // Walk parents to the root (guard against cycles with a generous cap).
    let mut path: Vec<&str> = vec![node];
    let mut cur = node;
    let mut guard = 0;
    while let Some(&p) = parent.get(cur) {
        path.push(p);
        cur = p;
        guard += 1;
        if guard > 64 {
            break; // pathological cycle — bail; truncation is flagged below
        }
    }
    path.reverse(); // root first (coarse → fine)

    let mut out = [0u8; TIERS];
    let mut truncated = path.len() > TIERS;
    for (depth, &n) in path.iter().enumerate().take(TIERS) {
        let siblings: &[&str] = if depth == 0 {
            roots
        } else {
            children.get(path[depth - 1]).map_or(&[][..], Vec::as_slice)
        };
        let idx = siblings.iter().position(|&s| s == n).unwrap_or(0);
        // 1-based so rank 0 is reserved for "no tier here"; saturate at 255.
        let rank = idx.saturating_add(1).min(u8::MAX as usize) as u8;
        if idx + 1 > u8::MAX as usize {
            truncated = true;
        }
        out[depth] = rank;
    }
    (out, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::from_ndjson;

    /// The exact shape `ruff_csharp_spo`'s harvester emits for one MedCare model.
    fn medcare_patient() -> Vec<Triple> {
        let ndjson = concat!(
            r#"{"s":"medcare:Patient","p":"rdf:type","o":"ogit:ObjectType","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient","p":"inherits_from","o":"medcare:DbBase","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient","p":"has_field","o":"medcare:Patient.kdnr","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient.kdnr","p":"rdf:type","o":"ogit:Property","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient","p":"has_function","o":"medcare:Patient.Save","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient.Save","p":"rdf:type","o":"ogit:Function","f":1.0,"c":0.9}"#,
            "\n",
        );
        from_ndjson(ndjson).expect("valid harvester ndjson")
    }

    #[test]
    fn facet_bytes_match_facetcascade_layout() {
        // part_of in hi (byte 5,7,9,…), is_a in lo (byte 4,6,8,…), classid LE.
        let f = Facet::from_parts(0xDEAD_BEEF, [0xAB, 0xCD, 0, 0, 0, 0], [0x01, 0x02, 0, 0, 0, 0]);
        let b = f.to_bytes();
        assert_eq!(&b[0..4], &[0xEF, 0xBE, 0xAD, 0xDE]); // classid LE
        assert_eq!(b[4], 0x01); // tier0 lo = is_a
        assert_eq!(b[5], 0xAB); // tier0 hi = part_of
        assert_eq!(f.part_of_chain(), [0xAB, 0xCD, 0, 0, 0, 0]);
        assert_eq!(f.is_a_chain(), [0x01, 0x02, 0, 0, 0, 0]);
        assert_eq!(f.facet_classid(), 0xDEAD_BEEF);
    }

    #[test]
    fn mints_every_structural_node() {
        let m = mint(&medcare_patient());
        // Patient, Patient.kdnr, Patient.Save, DbBase, ogit:{ObjectType,Property,Function}
        for n in [
            "medcare:Patient",
            "medcare:Patient.kdnr",
            "medcare:Patient.Save",
            "medcare:DbBase",
            "ogit:ObjectType",
            "ogit:Property",
            "ogit:Function",
        ] {
            assert!(m.facet(n).is_some(), "node {n} should be minted");
        }
        // Literals never become nodes.
        assert!(m.facet("true").is_none());
    }

    #[test]
    fn part_of_children_share_their_parents_prefix() {
        let m = mint(&medcare_patient());
        let patient = m.facet("medcare:Patient").unwrap();
        let kdnr = m.facet("medcare:Patient.kdnr").unwrap();
        let save = m.facet("medcare:Patient.Save").unwrap();
        // kdnr and Save are both part_of Patient → their part_of chains share
        // Patient's tier-0 rank (the prefix-routability invariant).
        assert_eq!(kdnr.part_of_chain()[0], patient.part_of_chain()[0]);
        assert_eq!(save.part_of_chain()[0], patient.part_of_chain()[0]);
        // …and Patient sits one tier shallower than its members.
        assert_eq!(patient.part_of_chain()[1], 0, "Patient is a part_of root → only tier 0 set");
        assert_ne!(kdnr.part_of_chain()[1], 0, "kdnr is one level deeper");
    }

    #[test]
    fn is_a_siblings_share_their_supertype_prefix() {
        let m = mint(&medcare_patient());
        // kdnr is_a Property, Save is_a Function — different supertypes, so their
        // is_a tier-0 ranks differ (distinct roots), while Patient is_a DbBase.
        let kdnr = m.facet("medcare:Patient.kdnr").unwrap();
        let save = m.facet("medcare:Patient.Save").unwrap();
        assert_ne!(kdnr.is_a_chain()[0], save.is_a_chain()[0]);
    }

    #[test]
    fn mint_is_injective_and_deterministic() {
        let triples = medcare_patient();
        let m = mint(&triples);
        // No two nodes collide on a facet (roundtrip-lossless address).
        let facets: BTreeSet<_> = m.iter().map(|(_, f)| f).collect();
        assert_eq!(facets.len(), m.len(), "facets are unique per node");
        for (n, f) in m.iter() {
            assert_eq!(m.node_of(f), Some(n), "reverse lookup is exact");
        }
        // Deterministic: a second mint of the same corpus is byte-identical.
        let m2 = mint(&triples);
        for (n, f) in m.iter() {
            assert_eq!(m2.facet(n), Some(f));
        }
        // This small corpus fits in 6 tiers — nothing truncated.
        assert!(m.truncated().is_empty());
    }

    /// A class with > 255 members is exactly what makes naive [`mint`] collide
    /// (the per-tier `u8` saturates). [`mint_factored`] must stay injective by
    /// cascading the address into a second tier.
    #[test]
    fn factored_mint_is_injective_past_the_255_cap() {
        // One class, 300 fields — over the single-tier cap.
        let mut tr = vec![Triple {
            s: "m:Big".into(),
            p: "rdf:type".into(),
            o: "ogit:ObjectType".into(),
            f: 1.0,
            c: 0.9,
        }];
        for i in 0..300 {
            let field = format!("m:Big.f{i:03}");
            tr.push(Triple { s: "m:Big".into(), p: "has_field".into(), o: field.clone(), f: 1.0, c: 0.9 });
            tr.push(Triple { s: field, p: "rdf:type".into(), o: "ogit:Property".into(), f: 1.0, c: 0.9 });
        }

        // Naive mint collides (the 300 members past index 255 saturate to 255).
        let naive = mint(&tr);
        let naive_distinct: BTreeSet<_> = naive.iter().map(|(_, f)| f).collect();
        assert!(
            naive_distinct.len() < naive.len(),
            "naive mint should collide past the 255 cap (this is the bug factored fixes)"
        );

        // Factored mint is injective and truncates nothing (2 tiers suffice).
        let m = mint_factored(&tr);
        let distinct: BTreeSet<_> = m.iter().map(|(_, f)| f).collect();
        assert_eq!(distinct.len(), m.len(), "factored mint must be injective past 255");
        assert!(m.truncated().is_empty(), "300 members fit in 2 base-255 tiers");

        // The members extend the class's part_of prefix (still prefix-routable).
        let big = m.facet("m:Big").unwrap().part_of_chain();
        let f000 = m.facet("m:Big.f000").unwrap().part_of_chain();
        let big_depth = big.iter().take_while(|&&b| b != 0).count();
        assert_eq!(&f000[..big_depth], &big[..big_depth], "member extends class prefix");
        // is_a tier-0 carries the bounded kind enum (1 = field), never a mega-root rank.
        assert_eq!(m.facet("m:Big.f000").unwrap().is_a_chain()[0], 1);
    }

    #[test]
    fn radix_codebook_answers_prefix_queries_without_a_graph_walk() {
        // 300-member class — the factored mint keeps it injective + prefix-routable.
        let mut tr = vec![Triple {
            s: "m:Big".into(),
            p: "rdf:type".into(),
            o: "ogit:ObjectType".into(),
            f: 1.0,
            c: 0.9,
        }];
        for i in 0..300 {
            let field = format!("m:Big.f{i:03}");
            tr.push(Triple { s: "m:Big".into(), p: "has_field".into(), o: field.clone(), f: 1.0, c: 0.9 });
            tr.push(Triple { s: field, p: "rdf:type".into(), o: "ogit:Property".into(), f: 1.0, c: 0.9 });
        }
        let m = mint_factored(&tr);
        let cb = m.radix_codebook(Axis::PartOf);
        assert_eq!(cb.axis(), Axis::PartOf);
        assert_eq!(cb.len(), m.len());

        // documentSymbol(Big) = the range under Big's part_of prefix = its 300
        // members (Big itself is at the shallower prefix, excluded by going one
        // tier deeper).
        let big = m.facet("m:Big").unwrap().part_of_chain();
        let big_depth = big.iter().take_while(|&&b| b != 0).count();
        let members = cb.under_prefix(big, big_depth);
        // Big + its 300 members all share Big's prefix; the members extend it.
        assert_eq!(members.len(), 301, "Big and its 300 members share the prefix");
        // depth 0 returns the whole codebook.
        assert_eq!(cb.under_prefix([0; TIERS], 0).len(), m.len());
    }

    #[test]
    fn b255_width_cascades_at_the_cap() {
        assert_eq!(b255_width(1), 1);
        assert_eq!(b255_width(255), 1); // 255 siblings fit one tier (digit 1..255)
        assert_eq!(b255_width(256), 2); // 256 needs a second tier
        assert_eq!(b255_width(640), 2);
        assert_eq!(b255_width(65025), 2); // 255^2
        assert_eq!(b255_width(65026), 3);
    }

    #[test]
    fn classid_resolver_is_applied() {
        // Stamp a stand-in classid (a real caller injects canonical_concept_id).
        let m = mint_with_classid(&medcare_patient(), |iri| {
            if iri == "medcare:Patient" { 0x0901 } else { 0 }
        });
        assert_eq!(m.facet("medcare:Patient").unwrap().facet_classid(), 0x0901);
        assert_eq!(m.facet("medcare:Patient.kdnr").unwrap().facet_classid(), 0);
    }
}
