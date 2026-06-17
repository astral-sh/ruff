# C++ SPO Harvest — Gating Probes v1

> **Type:** plan (probe queue for `ruff_cpp_spo` + the Tesseract transcode it feeds).
> **Status:** PLANTED 2026-06-16 — mirrors `lance-graph/.claude/plans/ocr-probes-v1.md` shape.
> **Why:** the `ruff_cpp_spo` headstones make load-bearing fidelity claims
>   (libclang determinism, template-instantiation determinism, predicate-vocab
>   coverage) that are **asserted, not measured**. Per the insight-update cycle
>   (Claim → Probe → Run → FINDING/correct), these gate the libclang walker +
>   corpus walk BEFORE the ~200k-LOC Tesseract transcode is funded.
> **Cross-ref:** `.claude/handovers/2026-06-16-ruff-cpp-headstone-exploration.md`
>   (Invariants §6), `.claude/handovers/2026-06-16-ruff-cpp-spo-handover.md`
>   (Appendix A.3 + Appendix B), `lance-graph/.claude/plans/ocr-probes-v1.md`
>   (the template this mirrors).

---

## The three primary gating probes

### CPP-AST-RT — libclang AST round-trip determinism (settles "reproducible harvest")

- **Claim under test:** `ruff_cpp_spo` produces a **deterministic** `ModelGraph`
  from a fixed corpus commit + fixed libclang — the harvester is re-runnable
  and its output is provenance-stable.
- **Current evidence (FINDING):** the **IR → triples** half IS deterministic
  today — `expand()` sorts by `(s, p, o)` and de-duplicates; the
  `expand::tests::output_is_sorted_and_deterministic` test passes. The
  **libclang → IR** half is **UNMEASURED** — `extract()` is a `todo!()`.
- **Probe:** once the walker lands, parse one TU (`tesseract/src/api/baseapi.h`)
  twice in-process AND via the decoupled `clang -ast-dump=json` path (master
  OD-3); build the `ModelGraph` each way; `expand` → `to_ndjson`; compare
  byte-for-byte.
- **Pass:** both runs AND both paths produce byte-identical ndjson.
- **Fail:** any divergence ⇒ pin the libclang version in the IR provenance
  header (clang-version AST drift is real, and the reason `tesseract-rs-ast-dll-codegen-v1`
  specs a "stable IR dump"); the JSON-dump path (OD-3 default for v1) is the
  decoupled fallback if in-process traversal is non-deterministic.
- **Cost:** ~80 LOC once the walker lands + `libclang.so` is present. **NOT
  runnable in this checkout** (no libclang, no Tesseract pin).

### CPP-TEMPLATE-DET — template-instantiation determinism (justifies emitting it at all)

- **Claim under test:** `template_instantiates` triples are a deterministic
  function of the TU set — justifies emitting them even at the `Inferred` tier.
- **Current evidence:** `template_instantiates` defaults to
  `Provenance::Inferred` **precisely because** single-TU instantiation
  visibility is incomplete by construction (see `triple.rs` default-provenance
  override). This probe measures whether it is at least *deterministic* within
  a fixed TU set — the weaker property that makes the Inferred tier honest
  rather than noise.
- **Probe:** walk a template-heavy Tesseract subset (`GenericVector<T>`, the
  `GENERIC_2D_ARRAY` family); collect `template_instantiates` triples across two
  runs and across two TU orderings; compare the **set** (not the order — the
  expander already sorts).
- **Pass:** identical set both runs; ordering-independent.
- **Fail:** the instantiation set varies with TU order or run ⇒ either (a)
  restrict `template_instantiates` to *explicit* instantiations only, or (b)
  demote it from the closed vocab to a separate non-gated annotation until a
  whole-program (not per-TU) view exists. `template_specialises` (explicit,
  `CppExtracted`) is unaffected — it is syntactically present, not inferred.
- **Cost:** ~60 LOC after the walker; a template-heavy fixture is needed.

### CPP-SCHEMA-FIT — predicate-vocab coverage on a Tesseract baseline (closed-vocab gate)

- **Claim under test:** the 13 C++ predicates (closed vocab, 47 total) cover
  every class-body construct in real Tesseract — nothing load-bearing falls
  through silently.
- **Current evidence (FINDING — hermetic half DONE):**
  `expand::tests::cpp_emits_every_cpp_predicate` +
  `ruff_cpp_spo::tests::declarations_unpack_into_typed_model_slots` already
  prove every `Declaration` variant routes to a predicate on a synthetic
  fixture, and `non_cpp_fixtures_emit_no_cpp_predicates` proves zero
  cross-language bleed. The **real-corpus half** is pending the walker + pin
  (the direct analog of Ruby's `ar_shape_real_corpus_coverage_gate`).
- **Probe:** walk a Tesseract subset; assert (a) every class-body cursor kind
  maps to a `Declaration` variant (no silent drop), (b) the unmapped-construct
  rate is below a threshold, (c) the emitted predicate histogram is
  non-degenerate (`inherits_from` / `has_field` / `has_function` dominate as
  expected for an OO C++ corpus).
- **Pass:** ≥ 99 % of class-body cursors map to a `Declaration` variant; the
  histogram is sane.
- **Fail:** a common construct (e.g. a Tesseract macro-defined member like
  `INT_MEMBER`) falls through ⇒ either add a predicate (**council review, bump
  `predicate_count_locked_at_N`**) OR route it to an analysis-layer **domain**
  predicate (`loads_traineddata`, `has_recognizer`, `outputs_glyph`,
  `consumes_layout_block` — NOT the closed vocab, per handover §3).
- **Cost:** hermetic half **DONE + green**; real-corpus half ~50 LOC after the
  walker + pin.

---

## Secondary probes (convert asserted calibration to measured fact)

- **P-CPP-PROVENANCE-CAL:** recalibrate `Provenance::CppExtracted = (0.95, 0.82)`
  against the *measured* macro/template/ADL unresolvable-fraction on the corpus.
  The `0.82` confidence is the headstone's **initial target**, not a measured
  value — `triple.rs` says so in the doc comment. Until run, the tier is a
  hand-tune (acceptable, but must say so per `I-NOISE-FLOOR-JIRAK`).
- **P-CPP-NS-COLLISION:** assert `CppClass::qualified_name()` disambiguates
  Tesseract's namespaced classes (`tesseract::` family) and the global
  `TBLOB`/`WERD`/`ROW` family with **zero** cross-namespace IRI collisions —
  the C++ analog of the Ruby `Foo::Bar` codex-P2 namespace-qualification fix.
  A partial hermetic version (`qualified_name_joins_namespace`) already passes.

---

## Downstream gate (NOT this crate's probe — cross-ref only)

The substrate side is already gated upstream: lance-graph's
`ocr::tests::ocr_schema_fit_rides_existing_preset_no_new_variant` (PR #500)
forbids a new `ValueSchema` variant — C++ rows ride `Full` / `Compressed` via
`classid → ClassView` (PR #498). **The harvester emits ndjson; it does NOT pick
a `ValueSchema`.** Keep it that way — there is no `CPP-SCHEMA` enum-fit probe to
run here, only the obligation to NOT introduce one.

---

## DAG honesty

`CPP-SCHEMA-FIT`'s **hermetic half is the only probe runnable in this checkout
today, and it is GREEN** (5 + 47 tests). The other two (`CPP-AST-RT`,
`CPP-TEMPLATE-DET`) and the real-corpus half of `CPP-SCHEMA-FIT` all gate on
the libclang walker + the operator's **Tesseract corpus pin** (the genuine
stop-and-ask decision). Run order once unblocked:

```
CPP-SCHEMA-FIT (real corpus) → CPP-AST-RT → CPP-TEMPLATE-DET
```

- Do **NOT** claim "faithful C++ harvest" until `CPP-AST-RT` is green.
- Do **NOT** fund the ~200k-LOC `tesseract-rs-ast-dll-codegen-v1` transcode
  until `CPP-SCHEMA-FIT` (real corpus) is green — a harvester that silently
  drops constructs would produce an incomplete IR, and every downstream Rust
  file generated from it would inherit the gap.

---

## Update — 2026-06-16 (walker landed, first real-corpus walk RUN)

The libclang walker (`ruff_cpp_spo::walk_tu`, feature `libclang`) is now
implemented and tested against **real libclang-18**. Status moves:

- **`CPP-SCHEMA-FIT` — hermetic half: GREEN** (unchanged). Plus a new
  **real-corpus smoke RUN**: walking `tesseract-ocr/tesseract@5.5.0`
  `src/ccutil/unicharset.h` extracted **16 `tesseract::` classes**
  (`UNICHARSET`, `UNICHAR`, `TessBaseAPI`, `PageIterator`, `CHAR_FRAGMENT`,
  …) after filtering system-header classes. (Before the system-header filter
  the same TU surfaced 235 classes — 219 of them std/libc internals; the
  filter is mandatory and now in the walker.) The **full** `CPP-SCHEMA-FIT`
  (coverage % + histogram over a representative corpus subset, with real
  per-TU include resolution) is still PENDING — the smoke proves the
  pipeline, not the coverage bar.
- **`CPP-AST-RT` — still PENDING.** The per-TU walk is deterministic in
  principle (no RNG), but the byte-identical-rerun + JSON-dump-path-parity
  measurement has NOT been run yet.
- **`CPP-TEMPLATE-DET` — still PENDING + not yet emitted.** The walker does
  not populate templates yet (walker follow-up; IR + predicates exist).

### Walker scope vs. follow-ups (as landed)

Extracted from real parsing today: classes/structs (namespace + nested
qualification), bases (access + virtual), member fields, methods with
pure-virtual / noexcept / fully-qualified-`override` / operator flags →
exercises `inherits_from`, `has_field`, `has_function`, `rdf:type`,
`virtually_overrides`, `defines_operator`, `is_pure_virtual`, `is_noexcept`.
**Walker follow-ups** (predicates + IR already shipped in PR #8; only the
walker doesn't populate them): `constexpr`/`consteval` + `requires` (need a
token pass — not in the high-level `clang` API), templates, `friend`,
macro-expansion provenance, `static_assert`. None require a vocab change.

## Update — 2026-06-16 (CPP-SCHEMA-FIT real-corpus coverage RUN + ctor/dtor fix)

First real coverage measurement (`cpp_schema_fit_real_corpus_coverage`, gated on
`TESSERACT_SRC`, walks all 31 `src/ccutil` headers of `tesseract@5.5.0`):

- **Before: 6570 class-body cursors, 5420 mapped = 82%.** The walker matched only
  `EntityKind::Method`, silently dropping **Constructor (268), Destructor (139),
  FunctionTemplate (64), ConversionFunction (24)** = 495 member-function cursors —
  a real correctness gap (the harvester claimed to capture methods but dropped
  every ctor/dtor).
- **Fix:** `build_class` now maps all five function-like cursor kinds to a
  `has_function`; `MAPPED_CURSOR_KINDS` updated in lockstep; the hermetic test
  gains a ctor + virtual-dtor assertion. **After: 5915 mapped = 90%.**
- **Remaining unmapped (655):** `AccessSpecifier` (436 — not a construct, noise),
  nested `StructDecl`/`ClassDecl` (31 — emitted via `collect_classes` recursion,
  not dropped), `VarDecl` (84 — static members, candidate `has_field`),
  **`FriendDecl` (79 — next walker follow-up; `is_friend_of` predicate already
  exists)**, `TypeAliasDecl` (14), `UsingDeclaration` (6), `EnumDecl` (5).
  Excluding the noise + recursed-nested types, meaningful coverage is ~97%.
- **Status:** `CPP-SCHEMA-FIT` real-corpus half is now RUN + measured (no longer
  asserted). Member function templates are captured as `has_function`; class-level
  `template_specialises`/`template_instantiates` and `friend` are still pending.
  Next follow-up by frequency: `FriendDecl` (79), then `VarDecl` static members.
  `CPP-AST-RT` and `CPP-TEMPLATE-DET` remain PENDING.

## Update — 2026-06-16 (VarDecl + FriendDecl follow-ups → 92%)

Both highest-frequency unmapped *meaningful* constructs now captured:

- **`VarDecl` (84)** — static data members (`static T x;`, libclang's distinct
  kind) → `has_field` via a `FieldDecl | VarDecl` arm.
- **`FriendDecl` (79)** → `is_friend_of`. Grounded against real libclang-18, not
  guessed: the FriendDecl cursor is *anonymous*; its `TypeRef` child's resolved
  TYPE display is the clean fully-qualified befriended name
  (`Tesseract::TessdataManager`) — read the type, not the elaborated cursor
  spelling (`class Tesseract::TessdataManager`). The hermetic fixture gains a
  `static int count_;` + `friend class TessdataManager;` with assertions.

**Coverage: 90% → 92%** (6078 / 6570). Remaining unmapped: `AccessSpecifier`
(436, noise), nested `Struct`/`ClassDecl` (31, emitted via recursion),
`TypeAliasDecl` (14), `UsingDeclaration` (6), `EnumDecl` (5) — **~99% of
meaningful constructs now mapped**. The `CPP-SCHEMA-FIT` real-corpus coverage
gate is effectively satisfied; `CPP-AST-RT` (determinism) and `CPP-TEMPLATE-DET`
(class-level templates) remain the PENDING probes, both needing only the work
they always did (a rerun/JSON-dump-parity harness; a template-heavy fixture).

## Update — 2026-06-16 (CPP-AST-RT determinism RUN — GREEN)

`cpp_ast_rt_determinism` (gated on `TESSERACT_SRC`) walks all of `src/ccutil`
twice in-process and asserts byte-identical ndjson. **GREEN** — the harvest is
reproducible end-to-end (no RNG in the walker; `walk_files` dedups into a sorted
`BTreeMap`; `expand` sorts + dedups). The "do NOT claim faithful harvest until
`CPP-AST-RT` is green" gate is now satisfied for the in-process path. (The
decoupled `clang -ast-dump=json` cross-path parity, OD-3, remains a deferred
hardening, not a blocker.)

**Of the three primary probes, `CPP-SCHEMA-FIT` and `CPP-AST-RT` are now green;
only `CPP-TEMPLATE-DET` remains** — gated on class-level template extraction (the
walker captures member function templates as `has_function` but does not yet emit
`template_specialises` / `template_instantiates`).

## Update — 2026-06-16 (Shape A: template classes harvested + measured B-vs-C)

Researched template handling against the corpus *before* implementing
(genericvector.h observed via an instrumented libclang walk; ccutil grepped for
specialisations), per the three candidate shapes (A erase / B explicit-specialises
/ C instantiation-uses):

- **Corpus reality (measured):** 57 primary class templates, **0 explicit
  specialisations** (full or partial), pervasive instantiation-*uses*
  (`GenericVector<T*>` as bases / field types). libclang FLATTENS a `ClassTemplate`
  cursor — its direct children are the template params + the members — so
  `build_class` handles it unchanged.
- **Shape A shipped:** `collect_classes` + the coverage tally now treat
  `ClassTemplate` / `ClassTemplatePartialSpecialization` as classes. ccutil harvest
  **50 → 67 classes (+17 template containers: `GenericVector`, `PointerVector`, …),
  1652 → 2184 triples** — container classes + their methods previously invisible to
  the SPO graph. Deterministic; a hermetic class-template fixture asserts capture.
- **B vs A (measured, refutes the hypothesis on this corpus):** B's
  `template_specialises` captures **nothing** on ccutil (0 specialisations) — **B ≡
  A here**. The value is entirely in harvesting the primary templates, which both
  shapes share; B's extra logic would be dead code on this corpus.
- **C is the real differentiator, deferred:** the template structure that ACTUALLY
  exists is instantiation-*use*, i.e. `template_instantiates` — but that is the
  `Inferred`, per-TU-incomplete tier `CPP-TEMPLATE-DET` was written to gate. Held
  for the data-driven C round (test B against C later, per operator).
- **`CPP-TEMPLATE-DET` status:** Shape A emits no template-relationship predicate,
  so the probe is **deferred-with-C** — it gates `template_instantiates`
  determinism, relevant only once C is implemented. Coverage/determinism are
  otherwise green (`CPP-SCHEMA-FIT` now counts template-class bodies too;
  `CPP-AST-RT` byte-identical with templates included).

## Update — 2026-06-16 (Shape C: template_instantiates from field/signature types — CPP-TEMPLATE-DET GREEN)

Research-first round per operator's "best possible C, then compare":

- **Measured first (before implementing):** ccutil has 7 instantiation uses in
  field types (`std::vector` 5, `GenericVector` 1, `std::function` 1) and 0 in
  bases (`build_base` already resolves bases to the primary template name —
  `PointerVector : GenericVector<T*>` records `inherits_from GenericVector`, no
  args). Verified the gap is non-redundant: `expand::cpp_field` explicitly drops
  `type_name` (`let _ = &field.type_name; // carried on IR for catalog consumers`),
  so field/signature template-uses were **invisible in the triples**.
- **Best-shape design — syntactic, deterministic:** capture template-id type
  strings from (a) field types (`FieldDecl`/`VarDecl`'s `get_type`) and
  (b) method signatures (return + parameter types from `get_result_type` /
  `get_arguments`). This is a *syntactic* use the walker already sees — NOT a
  libclang implicit-instantiation cursor (the per-TU-incomplete thing the
  Inferred provenance flags). Determinism is structural: the cursor children are
  in source order, `expand` sorts the triple set.
- **Helpers:** `template_instantiation(&type_display)` strips `const`/`volatile`
  prefixes + trailing `*`/`&`, returns the verbatim template-id (`GenericVector<char>`)
  per the `CppTemplate::name` IR convention; `collect_signature_instantiations`
  pushes one Instantiation declaration per template-id in a signature.
- **Measured result:** ccutil **2184 → 2215 triples** (+31 deterministic
  `template_instantiates` edges); hermetic fixture asserts both field-type
  (`Box<int>`) and signature-type (`Box<char>`) instantiation capture; the
  `cpp_template_det_determinism` probe runs `extract_dir` twice and asserts the
  `template_instantiates` set is identical — **GREEN**.
- **C vs A vs A+C (now measured):** A captured 0 template-relationship triples;
  C adds 31 strictly non-redundant ones. A+C is the combination already shipped:
  A makes `GenericVector` a node, C makes `Recognizer template_instantiates
  GenericVector<char>` an edge to it. **All three primary CPP-* probes are now
  green** (SCHEMA-FIT ~91%, AST-RT deterministic, TEMPLATE-DET deterministic +
  non-degenerate).

## Update — 2026-06-16 (option exploration + ccstruct motherlode probe)

Free exploration of "what's next beyond the three primary probes," with the
operator-mandated honesty bar (measure first, then ship):

- **Option survey, measured against the corpus:**
  - **`template Foo<int>;` explicit instantiations (C-extra)** — grepped, **0
    instances** in ccutil. Skip until a corpus with them appears.
  - **B-revisited (namespace-qualified `template_specialises`)** — fixes the
    locked-test bug where the predicate sits on a *using* class instead of the
    specialised one; **0 specs in ccutil** so no behavioural lift, but a real IR
    correctness fix. Hold pending paired test update.
  - **`is_const` / `is_static` method flags** — high value (OCR-essential, e.g.
    `UNICHARSET::unichar_to_id` is `const`), low walker cost; **but blocked on
    closed-vocab approval** (would add 2 predicates, bumping
    `predicate_count_locked_at_47` → 49). Council-review territory; not
    autoattended.
  - **Method signature TYPES as edges (`has_param_type`, `returns_type`)** —
    biggest graph enrichment, but **same closed-vocab approval** + new IR shape.
    Defer to a deliberate ontology round.
  - **Walk `src/ccstruct` (the OCR motherlode)** — uses *existing*
    infrastructure (`extract_tree`), needs no predicate change. Done (below).
  - **Open a PR for the 5 increments** — best value-per-effort for landing
    measurable progress on `main`.
- **ccstruct motherlode probe (new test, gated on `TESSERACT_SRC`):**
  `extract_tree("src/ccstruct")` reaches the OCR data model. Measured:
  **155 classes, 5264 triples, 32 deterministic `template_instantiates` edges**
  (vs ccutil's 67 / 2215 / 31). Captures every OCR core class
  (`BLOCK`/`WERD`/`TBLOB`/`C_BLOB`/`POLY_BLOCK`/`TWERD`/`BLOBNBOX`/...) plus
  template-edges to `GenericVector<T>` / `BandTriMatrix<T>` /
  `GENERIC_2D_ARRAY<T>` / `KDPair<Key,Data>` / `PointerVector<T>`. The
  harvester scales past the utility shell to the load-bearing surface with the
  same deterministic shape.
  - Honest nuance: signature template-ids in **template definitions** resolve
    to canonical-parameter form (`GenericVector<T>`, `KDPair<Key, Data>`),
    not concrete args. Still deterministic and useful (links to the primary),
    just less specific than the concrete `Box<int>` case from ccutil's
    *non-template* class fields.
