# Session prompt — ruff_cpp_spo + Tesseract transcode

> **Posted simultaneously to:**
> - `AdaWorldAPI/ruff/.claude/prompts/2026-06-16-cpp-spo-tesseract-session-prompt.md`
> - `AdaWorldAPI/tesseract-rs/.claude/prompts/2026-06-16-cpp-spo-tesseract-session-prompt.md`
>
> **Whichever repo you opened in, the same prompt orients you. Cross-links resolve both directions.**

---

## You are starting work on

Building **`ruff_cpp_spo`** in `AdaWorldAPI/ruff` (C++ SPO harvester via libclang) and driving the codegen pipeline that lands **generated Rust** into `AdaWorldAPI/tesseract-rs` (the Rust target — NOT a place to vendor C++). The previous tesseract-rs attempt was retired for the wrong shape (no ruff, no IR, C++ vendored inside the Rust target + FFI wrapper on top). You are doing it correctly.

---

## Read in this order (ground truth)

1. **The headstone in the repo you're in:**
   - if `ruff`: `.claude/handovers/2026-06-16-ruff-cpp-headstone-exploration.md`
   - if `tesseract-rs`: `.claude/handovers/2026-06-16-tesseract-rs-headstone-exploration.md`
2. **The sibling repo's headstone** (cross-linked at the end of yours).
3. **The tactical companions** (both repos):
   - `ruff`: `.claude/handovers/2026-06-16-ruff-cpp-spo-handover.md` — parser-library evaluation + scaffold proposal.
   - `tesseract-rs`: `.claude/handovers/2026-06-16-cpp-spo-corpus-handover.md` — corpus framing + post-#500 corrections.
4. **The upstream codegen plan** in lance-graph: `AdaWorldAPI/lance-graph/.claude/plans/tesseract-rs-ast-dll-codegen-v1.md` — the direct downstream consumer of your `ruff_cpp_spo` IR.
5. **The transcode master plan**: `AdaWorldAPI/lance-graph/.claude/plans/tesseract-rs-transcode-master-v1.md` — v2 transcode roadmap (LSTM hosted via `embedanything → candle → ndarray` AMX; layout 1:1 transcoded).
6. **The OCR canonical-SoA integration**: `AdaWorldAPI/lance-graph/.claude/plans/ocr-canonical-soa-integration-v1.md` + `ocr-probes-v1.md` — your gating-probe template.

If `lance-graph` PR #500 is still open when you start, read it too — it propagates `HelixResidue` 48 B → 6 B everywhere and pins the no-new-variant contract test.

---

## What's already in scope (do not re-litigate)

- **The harvester family pattern** (`AdaWorldAPI/ruff` PRs #1-#5): `language-specific parser → frontend-local IR → ModelGraph → expand() → Vec<Triple> → ndjson → lance-graph SPO store`. The Python (`ruff_python_dto_check`) and Ruby (`ruff_ruby_spo`) frontends are the structural templates.
- **The 4-layer architecture** from `lance-graph` PR #496: SurrealDB orchestrates AST → `lance-graph-planner` coordinates → `thinking-engine > P64 > cognitive-shader-driver` thinks → `callcenter` writes. C++ harvester output is a SurrealDB-orchestrated input.
- **`classid → ClassView` resolution** (`lance-graph` PR #498). Per-class `ValueSchema` preset selection without new variants.
- **`ValueSchema` presets** (PR #496) are closed: `Bootstrap` / `Cognitive` / `Compressed` / `Full`. **Do not propose a 5th.**
- **The previous tesseract-rs revert was about mechanism, not goal.** Read the operator clarifications in the tactical handovers' "Correction note" sections.

---

## What's NOT yet started (your work)

- **`ruff_cpp_spo` crate scaffold** — sibling to `ruff_ruby_spo` in this repo's `crates/` directory.
- **Locked-shape test** — hand-build a `Tesseract::Recognizer` `ModelGraph` + assert `expand()` output. Mirrors `ruff_ruby_spo::tests::locked_shape_expands_to_expected_triples`. Land this *before* wiring libclang.
- **`clang` crate wiring** — `Cargo.toml` dep + minimal walker that walks one Tesseract translation unit (e.g. `tesseract/src/api/baseapi.h`) and produces the locked shape.
- **Predicate vocab extension** — C++-flavored predicates (see `ruff_cpp_spo-handover.md` §3 and `ruff_cpp_headstone.md` "closed-vocab predicate set when complete" for the target list). Land them in `ruff_spo_triplet::Predicate` under the `predicate_count_locked_at_N` gate.
- **`Provenance::CppExtracted`** — calibrated against a Tesseract corpus baseline; initial target `(0.95, 0.82)`.
- **`tesseract-rs` cleanup** — any C++ source still vendored from the previous attempt must be removed. Salvageable Rust glue (if any) preserved under `legacy/`.
- **First codegen run** — once `ruff_cpp_spo` emits IR for a Tesseract subset, the lance-graph codegen plan consumes it and produces Rust files into `tesseract-rs`.

---

## Iron rules (fail loudly if violated)

1. **No C++ source inside `tesseract-rs`.** Single most concrete rule. The previous attempt failed it. `-rs` is a Rust target. C++ corpus stays at `tesseract-ocr/tesseract` upstream (or a pinned external corpus location).
2. **No new `ValueSchema` enum variant.** Per `lance-graph` PR #500's enforced contract test `ocr_schema_fit_rides_existing_preset_no_new_variant`. C++ rides `Full` / `Compressed` via `classid → ClassView`.
3. **`HelixResidue` width is 6 bytes** (the stored `Signed360` place index). Pre-#498 docs may say 48; that's a bits-bytes slip. Don't propagate.
4. **Locked-shape test before parser wiring.** `ruff_ruby_spo` PR #4 is the template — hand-build the `ModelGraph`, assert `expand()`, then wire `clang` crate.
5. **`ruff_spo_triplet` stays serde-only.** The libclang dependency lives in `ruff_cpp_spo`, never in the shared core.
6. **No re-parsing of source in `extract()`.** Frontend-local declarations get unpacked into shared `ModelGraph` slots via `unpack_declaration`. Mirror PR #5's discipline.
7. **`predicate_count_locked_at_N`** test enforces closed-vocab. Adding a variant without updating `Predicate::ALL` fails loudly. Land it for C++ extensions too.
8. **No model identifier in committed artifacts** (per workspace canon).

---

## First moves (suggested sequence)

1. **Check the state of both repos.** What's already in `ruff/crates/ruff_cpp_spo/`? What's in `tesseract-rs/` from the previous attempt? Inventory before changing anything.
2. **Decide the corpus pin.** Tesseract upstream `tesseract-ocr/tesseract` HEAD or a pinned tagged release. Record the decision in the codegen plan.
3. **`tesseract-rs` cleanup** if any C++ source from the previous attempt remains. PR with explicit *"retiring previous-attempt C++ vendoring per the headstone discipline"* commit message. Either preserve Rust glue under `legacy/` or remove cleanly.
4. **`ruff_cpp_spo` scaffold** — new crate with `Cargo.toml`, `lib.rs`, `extract.rs` (with `todo!()` at parser plug-in), `tests/` with the locked-shape test.
5. **Hand-build a `ModelGraph`** for `Tesseract::Recognizer` (or similar representative class) with 5-10 declarations. Assert `expand(&graph)` output. Test should pass before libclang wiring.
6. **Wire `clang` crate** as the parser. Walk one Tesseract TU. Compare output against the locked shape.
7. **Predicate vocab expansion** — add the C++-flavored predicates under the locked-count gate.
8. **First ndjson emission** — walk a Tesseract subset, emit to a file, load into `lance-graph` SPO store.
9. **Hand-off to the codegen plan** — `lance-graph`'s `tesseract-rs-ast-dll-codegen-v1` plan consumes your IR and produces Rust source.
10. **Spec the gating probes** — `CPP-AST-RT` (libclang AST round-trip), `CPP-TEMPLATE-DET` (template instantiation determinism), `CPP-SCHEMA-FIT` (predicate vocab coverage). Don't claim FINDING-grade fidelity without these.

---

## When to stop and ask

These are genuine operator-only decisions:

- **The Tesseract release pin.** Don't guess; ask.
- **Cleanup vs preservation of any pre-existing `tesseract-rs` content.** If there's anything non-trivial, ask before deleting.
- **Whether to add a probe / `Provenance` tier / new predicate** that doesn't have a direct precedent in Python or Ruby vocab. Ask before extending.
- **Hand-off boundary with `tesseract-rs-ast-dll-codegen-v1`.** Does `ruff_cpp_spo` stop at IR + triples emission (as `ruff_ruby_spo` does), or also drive the codegen step (as `ruff_python_dto_check` does)?
- **If `ocrs + rten` un-parks** during your session and there's an apparent overlap — confirm with the operator that the two paths still coexist; they're orthogonal but a fresh operator decision may reshape priorities.

---

## Discipline reminders (from prior session's failure modes)

- **Search the workspace, not just your transcript.** When you can't recall a term, grep the repos first. The transcript is a slice; the workspace is canon.
- **Verify the substrate before citing a name.** Don't claim *"X inherits Y's property"* without checking that X runs Y's algorithm. Shared bit-width is not shared algorithm.
- **Before any new column / tenant / variant proposal:** check whether the substrate has institutionalised *"no new variant"* (e.g. `ocr_schema_fit_rides_existing_preset_no_new_variant`). If yes, ride existing presets.
- **Five-specialist drift-catching pass** (`cascade-architect` / `family-codec-smith` / `palette-engineer` / `dto-soa-savant` / `truth-architect`) before any FINDING-grade claim. The substrate session uses this against its own plans; mirror it before you ship.
- **Gating probes before FINDING.** Spec the measurement before claiming the property.

---

## Outputs expected at end of session

At minimum:
- `ruff/crates/ruff_cpp_spo/` exists, compiles, passes its locked-shape test, includes scaffold for libclang wiring (with `todo!()` markers if libclang isn't wired yet).
- `ruff_spo_triplet::Predicate` has the C++ predicate variants added under the `predicate_count_locked_at_N` gate, with `default_provenance` calibration.
- `tesseract-rs` has any pre-existing C++ source from the previous attempt removed (or explicitly carved out under `legacy/` with a `DEPRECATED.md`).
- A PR opened in each repo for the additions; commit messages cite the relevant headstones + tactical handovers.

Stretch:
- First ndjson emission from a Tesseract subset.
- First codegen run via `lance-graph`'s `tesseract-rs-ast-dll-codegen-v1` plan producing at least one transcoded Rust file into `tesseract-rs`.

If neither stretch lands: that's fine. The minimum outputs unblock the next session.

---

## What you are NOT for

- **You are not building a runtime OCR engine.** That's `ocrs + rten` in a separate path. Don't reach for it; don't replace it.
- **You are not vendoring Tesseract C++ inside `tesseract-rs`.** The previous attempt's structural failure. The corpus stays upstream.
- **You are not hand-writing safe-Rust wrappers around the Tesseract C++ API.** That was the reverted mechanism. The correct shape generates Rust from the harvested IR via the codegen plan.
- **You are not extending `ruff_spo_triplet` with non-serde dependencies.** The shared core stays minimal.

---

## Cross-references

### Headstones (durable synthesis)
- `AdaWorldAPI/ruff/.claude/handovers/2026-06-16-ruff-cpp-headstone-exploration.md`
- `AdaWorldAPI/tesseract-rs/.claude/handovers/2026-06-16-tesseract-rs-headstone-exploration.md`

### Tactical companions (evaluation + first steps)
- `AdaWorldAPI/ruff/.claude/handovers/2026-06-16-ruff-cpp-spo-handover.md`
- `AdaWorldAPI/tesseract-rs/.claude/handovers/2026-06-16-cpp-spo-corpus-handover.md`

### Upstream plans (lance-graph)
- `.claude/plans/tesseract-rs-ast-dll-codegen-v1.md` — direct consumer of your IR
- `.claude/plans/tesseract-rs-transcode-master-v1.md` — v2 master roadmap
- `.claude/plans/ocr-canonical-soa-integration-v1.md` — OCR SoA wiring (the analog to what you produce)
- `.claude/plans/ocr-probes-v1.md` — gating-probes template

### Upstream PRs
- `AdaWorldAPI/lance-graph` PR #494 — `EdgeCodecFlavor` (the edge analog of `ValueSchema` for the codec selector pattern)
- `AdaWorldAPI/lance-graph` PR #496 — `ValueSchema` presets + §0 anti-invention guardrail
- `AdaWorldAPI/lance-graph` PR #497 — Tesseract → tesseract-rs 1:1 transcode v2 plans
- `AdaWorldAPI/lance-graph` PR #498 — GUID decode→read-mode keystone + helix Signed360 right-size + OCR→NodeRow transcode
- `AdaWorldAPI/lance-graph` PR #500 (status TBD at session start) — rebaseline + gating probes + no-new-variant contract test

### Ruff harvester family precedent
- `AdaWorldAPI/ruff` PR #2 — `ruff_python_dto_check`
- `AdaWorldAPI/ruff` PR #3 — AST → contract → codegen pipeline
- `AdaWorldAPI/ruff` PR #4 — `ruff_spo_triplet` + `ruff_ruby_spo` scaffold (closest structural template)
- `AdaWorldAPI/ruff` PR #5 — predicate vocab 7 → 34

---

_Authored by an external session (`AdaWorldAPI/bardioc` `session_01VysoWJ6vsyg3wEGc5v7T5v`). Posted symmetrically to both repos so a session opening in either lands on the same orientation. Update or supersede this prompt as the work progresses; keep the cross-link symmetry intact._
