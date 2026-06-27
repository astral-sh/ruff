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
**The `medcare_probe` example falsified that** against a real Roslyn harvest of
the MedCare C# corpus (9 199 nodes):

| measure | result |
|---|---|
| nodes / classes | 9 199 / 162 |
| **collisions** (injectivity) | **730** — `MainForm.ribbonBar5 == MainForm.ribbonButton1` |
| **truncated** | **8 525 / 9 199 (93 %)** |
| prefix-routable part_of | 9 305 / 9 305 ✓ |
| `documentSymbol` recovery | 640 / 640 ✓ |
| is_a redundancy | 92.5 % |

Cause (one mechanism, two shapes): the per-tier rank is a `u8`, so **God-classes**
(`mod_f_2_7_sono_gelenkStatus` 640 members, `MainForm` 330) and especially **flat
is_a roots** (`ogit:Property` 6 272 children, `ogit:Function` 2 748) overflow the
255-sibling cap and saturate to a shared rank → collisions. `documentSymbol` and
prefix-routing still hold because they only need the *coarse* prefix, which never
saturates.

The fix is not a bigger int — it is that a member's **kind** (Property/Function)
belongs in its `facet_classid`, not in a 6-tier sibling rank under a mega-root.
That is the same move the classid-gate proposes, and the 92.5 % is_a redundancy
is the evidence for it. See the `medcare_probe` [G] section (the 256-cap-is-a-lint law).

Reproduce:

```sh
dotnet run --project crates/ruff_csharp_spo/harvester -- /path/to/MedCare out.ndjson
cargo run -p ruff_spo_address --example medcare_probe -- out.ndjson
```

## Where it sits

This is brick 2 of the AST-as-`(part_of:is_a)`-address pipeline
(lance-graph `.claude/knowledge/ast-as-partof-isa-address.md`): brick 1 is the
`ruff_*_spo` harvest, brick 3 is the `medcare_probe` example (harvest → mint →
SoA → `documentSymbol`/prefix query; MedCareV2 reserved as the parity oracle).
The per-tier `FacetCascade` layout is locked upstream, so the minter invents no
new type — it writes into existing tiers.
