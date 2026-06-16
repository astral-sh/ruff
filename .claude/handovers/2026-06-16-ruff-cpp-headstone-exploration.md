# ruff C++ Headstone Exploration — AdaWorldAPI/ruff

## Purpose

This document is a headstone exploration for the full line of thought connecting:

```text
ruff_python_ast / ruff_python_parser
ruff_python_dto_check     (Python control plane)
ruff_ruby_spo             (Ruby class plane, lib-ruby-parser)
ruff_cpp_spo              (C++ machine plane, libclang) — THE NEW CRATE
ruff_spo_triplet          (closed-vocab grammar, ModelGraph IR)
ndjson over the AST→contract→codegen pipeline (PRs #2/#3)
lance-graph SPO store     (consumes ndjson via classid→ClassView)
tesseract-rs-ast-dll-codegen-v1   (transcode plan, lance-graph)
AdaWorldAPI/tesseract-rs  (Rust target — receives generated Rust ONLY)
```

The goal is to preserve the architectural synthesis before implementation details scatter it into separate plans.

---

## Capstone thesis

```text
ruff_python_dto_check parses the Python control plane.
ruff_ruby_spo          parses the Ruby class plane.
ruff_cpp_spo           parses the C++ machine plane.
ruff_spo_triplet       emits the shared SPO grammar — one Triple type, one closed Predicate vocab, one ModelGraph IR.
ndjson is the wire.
lance-graph accepts it.
classid is ground truth.
The closed-vocab gate is the integrity contract.
The five-specialist pass is the drift-catcher.
The locked-shape test is the substrate handshake.

Tesseract is the canonical corpus.
The transcode is the test.
tesseract-rs is the Rust target that receives generated source — never the C++ corpus.
```

---

## The three-layer architecture

### Layer 0 — Source corpus (upstream, never vendored)

C++ corpora live at their upstream homes. For Tesseract: `tesseract-ocr/tesseract` (or a pinned tagged release). For LLVM / Boost / OpenCV: their respective upstreams. The harvester points at a configurable path or a pinned commit; the corpus is **never copied into `AdaWorldAPI/tesseract-rs`** (the previous attempt's most concrete structural failure, retired by `lance-graph` PR #498).

This layer answers:

```text
what C++ source the harvester is walking
which commit / tag pins the corpus
where the corpus physically lives (never inside *-rs repos)
```

### Layer 1 — Harvest plane (this repo, `ruff_cpp_spo`)

Sibling crate to `ruff_python_dto_check` and `ruff_ruby_spo`. Uses `clang` crate (libclang FFI) as the canonical AST parser — the same architectural role `ruff_python_ast` plays for Python and `lib-ruby-parser` plays for Ruby. Walks the corpus, produces a frontend-local IR:

```text
CppClass {
  namespace, name,
  declarations: Vec<Declaration>
}

Declaration ∈ {
  Method, Constructor, Destructor, Field, StaticMethod,
  TemplateSpecialisation, TemplateInstantiation,
  VirtualOverride, Friend, Operator, UsingDecl, TypeAlias,
  PureVirtual, Constexpr, RequiresConcept, StaticAssert
}
```

`extract(path) -> ModelGraph` unpacks declarations into the shared `ModelGraph` sibling fields. **No re-parsing of source in `extract()`.** This layer answers:

```text
what classes exist in the corpus
what methods / fields / templates / overrides each class declares
how virtual / friend / template structure binds them
what the resolved type of each declaration's signature is
what macro expansions, namespaces, ADL contribute
```

### Layer 2 — Substrate plane (downstream, lance-graph)

`ruff_spo_triplet::expand(&ModelGraph) -> Vec<Triple>` emits the closed-vocab SPO grammar. `to_ndjson` writes the wire format. `lance-graph` ingests it, stores in the SPO graph, and `classid → ClassView` resolves which `ValueSchema` preset materialises the value slab for each emitted row. The substrate's existing `EdgeCodecFlavor` (PR #494) + `ValueSchema` (PR #496) + `ReadMode` (PR #498) + `OcrProvider` engine-agnostic boundary (PR #498) all hold without amendment.

This layer answers:

```text
which SPO triples got emitted for this C++ class
how they compose with Python / Ruby triples in the same graph
which classid each triple's subject resolves to
which ValueSchema preset the consumer reads against
how downstream tesseract-rs-ast-dll-codegen-v1 picks up the IR to emit Rust
```

---

## Why `ruff_python_dto_check` and `ruff_ruby_spo` alone are not enough

The Python and Ruby harvesters give SPO coverage of *control planes* (Flask routes, Rails models, ActiveRecord shapes). What they cannot see is the *machine plane* — the C++ that implements the recognizers, the kernels, the OCR engines, the linear algebra primitives that the high-level frameworks compose. Without `ruff_cpp_spo` the substrate's SPO graph has a load-bearing hole where the C++ machinery lives.

C++ adds three concerns the Python / Ruby parsers don't:

- **Templates** — two-phase lookup; partial / explicit specialisation; instantiation visibility per TU.
- **Preprocessor** — macro expansions affect identifier provenance; some declarations exist only post-expansion.
- **ADL** — argument-dependent lookup means the binding of `f(x)` depends on the type of `x` in a way only a semantic AST can resolve.

These cannot be approximated. They require a parser with the type system in scope.

---

## Why `bindgen`, `autocxx`, `tree-sitter-cpp`, `cppast` alone are not enough

Each does one piece; none produce SPO-shaped output that composes with the substrate's classid-resolved consumption:

| Tool | What it produces | Why insufficient |
|---|---|---|
| `bindgen` | Rust FFI bindings | Not SPO; FFI surface only; opaque template handling |
| `autocxx` | Safe C++ bindings | Not a harvester; consumes C++ to produce Rust callers, doesn't extract structure |
| `tree-sitter-cpp` | Syntax tree | Purely syntactic — templates, macros, ADL unresolved |
| `cppast` | Semantic AST | **Archived 2022** — do not build on |
| `Boost.Wave` | Preprocessor | Macro-expansion subproblem only |
| CHIR / MLIR | Compiler IR | Wrong altitude; research-grade, unstable |

The synthesis `ruff_cpp_spo` provides is the *composition*: libclang for semantic resolution + `ruff_spo_triplet` for closed-vocab emission + `ndjson` for substrate ingest + `classid → ClassView` for consumer dispatch. No single one of the above libraries gives the full chain.

---

## The closed-vocab predicate set when complete

The Python vocab (~30 emit categories) and Ruby vocab (34 predicates after PR #5) establish the discipline. C++ adds these, gated by `predicate_count_locked_at_N` per PR #5:

| Predicate | Captures |
|---|---|
| `inherits_from` | Single + multiple inheritance; access specifier in object slot |
| `template_specialises` | Explicit specialisation (partial + full) |
| `template_instantiates` | Materialised instantiation visible in TU |
| `virtually_overrides` | `override` keyword + virtual base method exists |
| `is_friend_of` | `friend class` / `friend fn` declarations |
| `defines_operator` | Operator overload (per operator-kind in object slot) |
| `uses_macro_expansion` | Identifier originates from macro expansion |
| `is_pure_virtual` | `= 0` declaration |
| `is_constexpr` / `is_consteval` | Compile-time computable markers |
| `is_noexcept` | Exception specification |
| `requires_concept` | C++20 `requires` clause |
| `static_asserts` | `static_assert` in class scope |
| `delegates_to` | Pimpl idiom; member-of-member calls |
| `has_field` | Member field declaration |
| `defines_method` | Free fn or method definition |

This is the *destination vocab*. The shape lock test (`predicate_count_locked_at_N` for C++-extended N) enforces no drift.

---

## Provenance calibration when complete

Per the `Provenance` discipline established in PR #4 / #5:

```text
Provenance::CppExtracted = (frequency, confidence)
```

Initial calibration target — to be measured on Tesseract corpus:

- **Frequency:** matches `OpenProjectExtracted = 0.95` (declarative C++ surface — class layouts, virtual tables, template structure — is structurally certain in the same sense `belongs_to :project` is).
- **Confidence:** below `OpenProjectExtracted = 0.88`; estimate `0.82` (templates + macros + ADL add a metaprogramming surface delta beyond Ruby's metaprogramming surface).

Per-edge overrides anticipated:

- `uses_macro_expansion` → `Inferred` (heuristic by definition — macro provenance loses some surface info).
- `template_instantiates` → `Inferred` when the instantiation is visible only in one TU (incomplete graph view).
- `is_friend_of` → `Structural` (declarative; no inference involved).

---

## Invariants

These are what the substrate enforces; `ruff_cpp_spo` inherits them from the workspace canon, not from this doc.

1. **Closed-vocab gate.** `predicate_count_locked_at_N` (PR #5). Adding a variant without updating `Predicate::ALL` fails loudly.
2. **Locked-shape test first.** Hand-build a `ModelGraph` for `Tesseract::Recognizer`; assert `expand()` output before wiring the parser. Mirror `ruff_ruby_spo::tests::locked_shape_expands_to_expected_triples`.
3. **No new `ValueSchema` variant.** Per `lance-graph` PR #500's enforced contract test `ocr_schema_fit_rides_existing_preset_no_new_variant`: the C++ harvester rides `Full` / `Compressed` presets via `classid → ClassView`. **Do not propose `ValueSchema::Cpp`.**
4. **No C++ source vendored into `tesseract-rs`.** The previous attempt's structural failure. `*-rs` repos hold Rust; the corpus stays upstream.
5. **Five-specialist drift-catching pass** before any FINDING-grade claim. Pattern from `lance-graph` #500.
6. **Gating probes before FINDING.** `CPP-AST-RT` (libclang round-trip determinism), `CPP-TEMPLATE-DET` (template instantiation determinism), `CPP-SCHEMA-FIT` (predicate vocab coverage on baseline). Pattern from `lance-graph/.claude/plans/ocr-probes-v1.md`.
7. **Provenance per-predicate-class by default, per-edge overrides for principled exceptions.**
8. **`ruff_spo_triplet` stays serde-only.** `ruff_cpp_spo` carries the libclang dep; the shared core does not.

---

## Why this lives in `ruff`, not in a new repo

The harvester family already lives here — `ruff_python_ast` / `ruff_python_parser` for Python, `ruff_python_dto_check` for Python DTO walking, `ruff_ruby_spo` for Ruby/Rails. `ruff_cpp_spo` is the C++ sibling. The shared SPO core (`ruff_spo_triplet`) is one level up from any single frontend.

`AdaWorldAPI/ruff` is also a fork of `astral-sh/ruff` — the upstream is the Rust-native Python linter. The fork's value is exactly the SPO-harvester family built on top of ruff's parser-as-library posture. C++ joining the family is the natural extension of that posture.

---

## What "complete" looks like

The headstone is reached when:

1. **`ruff_cpp_spo` walks Tesseract source via libclang** and produces a `ModelGraph` that `ruff_spo_triplet::expand` emits as ndjson without errors.
2. **`lance-graph` SPO store ingests the ndjson** and resolves every triple's subject via `classid → ClassView` to materialise correctly.
3. **`tesseract-rs-ast-dll-codegen-v1` consumes the same IR** to emit Rust source files into `AdaWorldAPI/tesseract-rs`, which contains *only generated Rust* (no C++).
4. **The gating probes pass:** `CPP-AST-RT`, `CPP-TEMPLATE-DET`, `CPP-SCHEMA-FIT` all green; the FINDING-grade claims about template-instantiation determinism and predicate-vocab coverage become measurable, not asserted.
5. **The closed-vocab gate holds** for at least one further harvester extension (e.g., a Java harvester `ruff_java_spo`) — proving the discipline is reusable, not just one-off for C++.

When these five hold, the workspace has SPO coverage of the full *language family* of the substrate's downstream consumers, and the C++ machine plane joins Python's control plane and Ruby's class plane in one graph.

---

## Headstone state — what the era closes

```text
The era that closes:
  - Hand-wrapping C++ libraries with ad-hoc unsafe FFI.
  - Vendoring foreign-language source inside *-rs repos.
  - Per-language re-invention of SPO-emission grammar.
  - "Tesseract is the wrong direction" as a category error (the mechanism was wrong; the goal was correct).

The era that opens:
  - libclang-mediated C++ AST harvest into ruff_spo_triplet IR.
  - Classid-resolved value schema per emitted row (no new variant per language).
  - Generated Rust into language-targeted repos; corpora stay upstream.
  - One SPO graph spanning Python control plane, Ruby class plane, C++ machine plane —
    all queryable through the same lance-graph substrate.
```

The capstone thesis at the top of this doc is the one-line restatement of the open-era state.

---

## Cross-references

### This repo (`AdaWorldAPI/ruff`)
- PR #1 — `woa_transcode_harvest` (ruff parser as library)
- PR #2 — `ruff_python_dto_check` (Python harvester)
- PR #3 — `[dto-check] AST→contract→codegen pipeline, lint-calibrated`
- PR #4 — `feat(spo): language-agnostic SPO triplet expansion + Ruby/Rails scaffold`
- PR #5 — `D-AR-1 + D-AR-2: OpenProject AR-shape predicate vocab + Model IR expansion` (vocab 7 → 34)
- `crates/ruff_spo_triplet/SPO_TRIPLET_EXTRACTION.md` — methodology
- `crates/ruff_python_dto_check/CODEGEN-DESIGN.md` — codegen pipeline design

### Companion handover in this repo
- `.claude/handovers/2026-06-16-ruff-cpp-spo-handover.md` — **tactical** parser-library evaluation, scaffold proposal, and open questions. This headstone exploration names the *destination shape*; the tactical handover names the *first steps*.

### Sibling cross-repo
- `AdaWorldAPI/tesseract-rs/.claude/handovers/2026-06-16-cpp-spo-corpus-handover.md` — what the previous tesseract-rs revert meant + corpus framing.

### Upstream architecture context
- `AdaWorldAPI/lance-graph` PR #496 — `ValueSchema` presets + §0 anti-invention guardrail.
- `AdaWorldAPI/lance-graph` PR #498 — `ReadMode { value_schema, edge_codec }` + classid → ReadMode registry + helix `Signed360` (6-byte place index) + OCR `LayoutBlock → NodeRow` transcode POC + `OcrProvider` engine-agnostic boundary.
- `AdaWorldAPI/lance-graph` PR #500 (open at time of writing) — rebaseline of #497 OCR plans; **enforced no-new-variant contract test**; 5-specialist drift-catching framing; gating probes pattern; HelixResidue 48 B → 6 B propagated.
- `AdaWorldAPI/lance-graph/.claude/plans/tesseract-rs-ast-dll-codegen-v1.md` — `clang → IR → Rust via ruff` codegen plan; the **direct downstream consumer** of `ruff_cpp_spo`'s IR.
- `AdaWorldAPI/lance-graph/.claude/plans/ocr-probes-v1.md` — gating probes template.
- `AdaWorldAPI/lance-graph/.claude/plans/ocr-canonical-soa-integration-v1.md` — OCR canonical-SoA wiring; the analog of what the C++ transcode produces.

### Other workspace headstones (for shape reference)
- `AdaWorldAPI/lance-graph/.claude/plans/3DGS-Cesium-BindSpace4-headstone-exploration.md` — the headstone shape this document follows.
- `AdaWorldAPI/bardioc/ROADMAP_RUST_PRIMARY_HEADSTONE.md` — Phase A→I migration headstone.

---

_Authored by an external session (`AdaWorldAPI/bardioc` `session_01VysoWJ6vsyg3wEGc5v7T5v`). Headstone shape — preserves the architectural synthesis. Companion tactical handover at `2026-06-16-ruff-cpp-spo-handover.md` carries the evaluation + open questions. No code, no PR — synthesis-preservation only._
