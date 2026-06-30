# ruff_spo_address

The deterministic **`(part_of:is_a)` rank-minter** — the brick between the
`ruff_*_spo` SPO harvest and the lance-graph `(part_of:is_a)` GUID SoA.

The carrier (`lance_graph_contract::facet::FacetCascade`, shipped) already
exists; this crate fills the **mint**: given a corpus's part_of edges
(`has_field` / `has_function`) and is_a edges (`inherits_from` / `rdf:type`),
it assigns every node a deterministic `(part_of_rank, is_a_rank)` at each of the
6 cascade tiers and packs them into a 16-byte facet whose bytes are
**layout-identical** to `FacetCascade`.

```text
source (C#/…) ──ruff_*_spo harvest──► SPO triples ──ruff_spo_address::mint──►
    (part_of:is_a) Facet ──► lance-graph (part_of:is_a) GUID SoA ──► ruff-lsp
```

## What it produces

```rust
use ruff_spo_address::mint;

let m = mint(&triples);                 // Vec<Triple> from ruff_*_spo
let f = m.facet("medcare:Patient").unwrap();
f.to_bytes();        // 16 bytes — feed to FacetCascade::from_bytes
f.part_of_chain();   // == FacetCascade::hi_chain  (mereology / documentSymbol)
f.is_a_chain();      // == FacetCascade::lo_chain  (taxonomy  / typeHierarchy)
```

- **Prefix-routable both ways.** Members of the same class share a leading
  `part_of_chain` prefix; subtypes of the same base share a leading
  `is_a_chain` prefix — so an LSP `documentSymbol` / `typeHierarchy` query is a
  longest-common-prefix over the cache-resident key column.
- **Exact below the per-tier cap, not a PQ approximation.** Ranks are a
  deterministic assignment (sorted sibling order) — roundtrip-lossless and
  injective *as long as every sibling set is ≤ 255 and depth ≤ 6*. Iron-rule
  clean per `I-VSA-IDENTITIES` (encodes identity positions, never bundles
  content). The cap is real and measured — see the fence below.
- **`facet_classid`.** `mint` leaves it `0`; `mint_with_classid` takes a
  resolver so a caller holding the OGAR codebook can stamp the canonical
  class-id (e.g. `lance_graph_contract::canonical_concept_id`) **BBB-safely** —
  this crate stays pure `std` + `ruff_spo_triplet`, never linking the codebook.

## Honest fence — MEASURED on a real corpus, not assumed

A node whose part_of/is_a depth exceeds 6 tiers (or a sibling set > 255) is
reported in `Mint::truncated()` — beyond the cap the facet is a routing prefix,
not a lossless address (deeper levels are the registry/ref-escape's job).

Earlier this doc claimed "for class graphs (depth ≈ 3–4) nothing truncates."
**That was falsified against a real multi-thousand-node corpus** (a Roslyn
harvest of a production C# codebase, run downstream via `ruff_csharp_spo`):
the naive [`mint`] produced real collisions and truncations once two
structures crossed the 255-sibling cap — a **God-class** (a single class with
hundreds of fields, the part_of axis) and a **flat is_a root** (a kind-
discriminator type with thousands of direct children, the is_a axis). Coarse
queries (`documentSymbol`, prefix routing) still held because they only need
the *coarse* prefix, which never saturates; only fine-grained identity
(injectivity) broke.

The fix is not a bigger int — it is that a member's **kind** (e.g.
Property/Function) belongs in its `facet_classid`, not in a 6-tier sibling
rank under a mega-root. That is the same move the classid-gate proposes.
[`mint_factored`] is the corrected minter: it builds is_a from
`inherits_from` only (the kind-discriminator mega-root never enters the
sibling rank) and gives part_of a base-255 positional path that cascades
deeper instead of saturating — both failure modes go to zero on the same
corpus that exhibited them.

## Where it sits

This is brick 2 of the AST-as-`(part_of:is_a)`-address pipeline
(lance-graph `.claude/knowledge/ast-as-partof-isa-address.md`): brick 1 is the
`ruff_*_spo` harvest, brick 3 is a downstream probe (harvest → mint → SoA →
`documentSymbol`/prefix query) against your own corpus. The per-tier
`FacetCascade` layout is locked upstream, so the minter invents no new type —
it writes into existing tiers.
