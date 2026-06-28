# OGAR Polyglot AST Integration (RFC)

**Status:** RFC / design contract. Not yet implemented or compile-verified —
this document locks the contract so the implementation + tests land as a
typed follow-up.
**Scope:** the `ruff_*_spo` frontends, the `ruff_spo_triplet` IR, the
`ruff_spo_address` mint, and a new `LangBackend` adapter family.
**Goal:** turn ruff's per-language SPO harvesters into a **bidirectional
polyglot AST substrate** — source (Python / C++ / C#) → OGAR IR → re-emitted
source (Python / Rust / C#) — with the IR as a content-addressed interlingua.

---

## 0 · Why this is mostly "fix an asymmetry," not "build a transpiler"

The interlingua already exists and is already bidirectional:

```
 SOURCE                       frontend                  IR (interlingua)              address
 ──────                       ────────                  ────────────────              ───────
 Python ─ ruff_python_dto_check ─▶ (JSON bundles) ─┐
 C++    ─ ruff_cpp_spo (libclang) ────────────────▶│
 C#     ─ Roslyn tool ─ndjson─ ruff_csharp_spo ───▶│  ModelGraph ─expand─▶ Triples ─mint─▶ 16B Facet
 Ruby   ─ ruff_ruby_spo (lib-ruby-parser) ────────┘  (ruff_spo_     ▲  │              (ruff_spo_address,
                                                      triplet::ir)  │  ▼               node_of = reverse)
                                                 reassemble ◀───────┘  ndjson ─▶ lance_graph SPO store
                                                      │
 TARGET ◀──── backend / adapter ◀──── ModelGraph ◀────┘
 ──────
 Rust   ◀─ ruff_cpp_codegen   (THE ONLY BACKEND TODAY: ModelGraph → Rust MethodSig source)
 Python ◀─ (none yet)
 C#     ◀─ (none yet)
```

Three load-bearing facts (verified against `main`):
1. **`ruff_spo_triplet::ir::ModelGraph` is the interlingua**, and it is
   already bidirectional — `expand` (ModelGraph→triples) ⇄ `reassemble`
   (triples→ModelGraph). "A new language is a new frontend, not a new
   ontology" (crate doc).
2. **The asymmetry is 4 frontends vs 1 backend.** `ruff_cpp_codegen` already
   proves `ModelGraph → Rust source` (it renders `MethodSig` manifests
   targeting `lance_graph_contract::codegen_manifest`). The superpower is
   *generalizing the back door*.
3. **The frontends do not enter uniformly:** C++/Ruby produce `ModelGraph`
   directly; Python produces JSON `ModuleHarvest` bundles; C# runs an
   out-of-process Roslyn tool → ndjson → `load() -> Vec<Triple>`.

---

## 1 · The IR record (Phase-0 contract — lock this first)

### 1.1 Dual-mode facet payload (classid-tagged)

`12 bytes = 96 bits` tiles **exactly** two ways. The 4-byte classid is the
discriminant.

```rust
// 16-byte content address; byte-identical to lance_graph_contract::facet::FacetCascade
pub struct Facet { facet_classid: u32, payload: [u8; 12] }

enum FacetMode {            // carved from facet_classid (mirrors TailVariant)
    Cascade,                // payload = [FacetTier; 6]  — 6 × (part_of:8, is_a:8)
    Triplet,                // payload = [SpoTriple; 4]  — 4 × (subject:8, predicate:8, object:8)
}
```

- **Cascade** = *position*: 6 tiers deep, predicate implied (`part_of`+`is_a`).
  Subsumption / containment as bit-ops. The address/index view.
- **Triplet** = *local edges*: 4 SPO edges, predicate explicit (256-way). The
  raw-graph view. **A `ruff_spo_triplet::Triple` IS a triplet-mode facet**
  (interned via the same `ruff_spo_address` mint) — this is what unifies the
  SPO corpus with the facet primitive (today they are separate substrates).

A cascade tier `(part_of:is_a)` is a degenerate triplet with its predicate
implied; triplet mode is the generalization (spend 2 tiers → buy an explicit
predicate).

### 1.2 The 512-byte record as 32 tenants

```
NodeRow 512B ≡ [Facet; 32]            (AoS row)
            ≡ 32 tenants × [GUID; N]  (SoA: each "tenant" = one GUID column)
   tenant 0      = Self GUID
   tenant 1      = Edges (EdgeBlock: in_family[12] | out_family[4])
   tenants 2..31 = 30 composition slots → GUID references to other classes
```

`ClassView::tenant_schema(classid) -> [TenantRole; 32]`, static per classid
(keeps every tenant a homogeneous, SIMD-scannable GUID column). Roles:
`{ Self, Edges, Structural, Do, Think, Adapter }` + `nested: bool`. The
`Do`/`Think`/`Adapter` tenants are the behaviour / cognitive / projection
planes reached *through* the classid, never inlined. Nesting = a content-
addressed foreign-key column → a columnar composition DAG with dedup-by-content.

### 1.3 Capacity == the separation-of-concerns lint
Every cap is a structural-quality signal, on both axes:
`>64 fields` (FieldMask) · `>256 per tier` / `>6 deep` (cascade) · `>4 edges`
(triplet) · `>30 composition slots` / `>32 tenants`. Overflow is **the
signal**; the fix is always "reference another class" (grow a limb), never
widen. The law already exists as a falsifier
(`ruff_spo_address/examples/medcare_probe.rs` §[G] "the 256-cap-is-a-lint
law", classifying overflow as DUPLICATION and/or CONFLATION) but is **not**
wired as a diagnostic — see Phase 2.

---

## 2 · Phases

### Phase 0 — Lock the IR contract
- Freeze `ModelGraph` + a **versioned closed `Predicate` registry**: a core
  agnostic set (`part_of`, `is_a`, `has_field`, `has_function`,
  `inherits_from`, `rdf:type` — the six the mint needs) + per-plane extension
  predicates, all under one registry with a conformance test.
- Lock the dual-mode `FacetMode` + the `[Facet; 32]` / tenant layout +
  `tenant_schema`.
- Fix the predicate-count doc-drift (comments say 34/53; code/test carry 57).

### Phase 1 — Normalize the three frontends to one `extract() -> ModelGraph`
| Lang | Today | Action |
|---|---|---|
| **C++** | `ruff_cpp_spo::extract() -> ModelGraph` (reference) | register its 13 predicates; widen corpus |
| **Python** | `ruff_python_dto_check` → JSON `ModuleHarvest`, not ModelGraph | add `bundle → ModelGraph` adapter (reuse extractors/matcher); the parse is done, only the IR shape is missing |
| **C#** | Roslyn `.NET` tool → ndjson → `load() -> Vec<Triple>`, `NAMESPACE="medcare"` hardcoded | generalize the harvester past MedCare; wrap `load → reassemble → ModelGraph` as a one-call `extract()`; document the out-of-proc Roslyn seam |
- **Gate:** each frontend round-trips `ModelGraph → expand → ndjson → reassemble`
  losslessly (`#[cfg(test)]` per crate).

### Phase 2 — Convergence + the SoC lint (guardrails)
- **Cross-language convergence:** the same construct in Python/C++/C# mints the
  **same `Facet`** — a CI test (the ruff analogue of
  `bridge_codebook_convergence`). This is the proof the IR is agnostic.
- **Promote §[G] → a real `ruff` diagnostic** (`OGAR-SOC`): on
  64-field / 256-tier / 6-deep / 4-edge / 30-slot overflow, emit the two-way
  verdict (`DUPLICATION → masked ClassView`; `CONFLATION → split data⊥behaviour,
  hoist constructor to compute_dag`). ruff is the linter; this is its home.

### Phase 3 — The backend / adapter family (the superpower)
Lift the one existing backend into a trait, mirroring OGAR's adapter pattern
(SurrealQL / ClickHouse / TTL) but targeting *source code*:
```rust
pub trait LangBackend {
    fn render(&self, g: &ModelGraph) -> String;   // ModelGraph → target source
}
```
- **Rust** ◀ `ruff_cpp_codegen` (exists) — generalize beyond C++ method manifests.
- **Python** ◀ extend the existing `ruff_python_codegen` (the formatter's
  generator) to render *from* ModelGraph, not just re-format ASTs — big leverage.
- **C#** ◀ new `ruff_csharp_codegen` (text emit, Roslyn-free).
- **Gate (round-trip conformance — what makes it a compiler substrate):**
  `source(L1) → ModelGraph → source(L2)`. `L1==L2` → defined normal form;
  `L1≠L2` → structural arm transpiles, behavioural arm is **flagged, not
  silently dropped**.

### Phase 4 — Land in the OGAR substrate
ndjson → `lance_graph` SPO store. **Firewall invariant:** the IR triples stay
the canonical artifact (in-memory / compile-time); only a *derived ANN index*
goes to Lance — code is never lowered to Lance rows. Guard with a conformance
test (today this holds partly by omission — the production Lance `SpoStore`
doesn't exist yet).

---

## 3 · The honest scope boundary
**Structure transpiles; behaviour does not.** `OGAR-AS-IR.md`: "the behavioural
arm cannot survive lowering and stays in the IR" — and tellingly the existing
backend renders `MethodSig` *signatures*, not method bodies. So Phase 3 is a
**schema / interface / DTO / ORM-model transpiler** across Python⇄Rust⇄C#
(API contracts, type defs, model shells — already enormous). Full *behaviour*
transpilation (method bodies → executable target logic) is a separate research
arm via `ActionDef` / `KausalSpec`, **not** Phase 3.

## 4 · Critical path
`Phase 0 → Phase 1 (Python + C# normalization) → Phase 3 (LangBackend +
Python backend)`. C++ is already both a frontend and (via `ruff_cpp_codegen`)
the backend reference, so it is the end-to-end smoke test on day one. This work
lives entirely in `ruff` + `OGAR`/`lance-graph` and does not touch any
consumer's parity trunk.
