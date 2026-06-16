# Handover: `ruff_cpp_spo` proposal — C++ parser library evaluation + Tesseract as corpus

> **Origin:** session in `AdaWorldAPI/bardioc` (`session_01VysoWJ6vsyg3wEGc5v7T5v`), 2026-06-16.
> **Status:** evaluation + scaffold proposal. No code yet. Not a PR — handover only so a future session in this repo can pick up with grounded context.
> **Why handed off:** the C++ harvester would belong here (sibling to `ruff_python_dto_check` and `ruff_ruby_spo`), but writing it from a bardioc session would (a) explode token usage and (b) drift outside bardioc's scope. Posting under `.claude/handovers/` so a session that actually owns this repo can act on it.
> **Companion handover** in `AdaWorldAPI/tesseract-rs/.claude/handovers/2026-06-16-cpp-spo-corpus-handover.md` — names the Tesseract-corpus angle and the reverted-runtime-direction context.

---

## 0. TL;DR

The Ruby harvester scaffold (`ruff_ruby_spo`, PR #4) and the predicate-vocab expansion (PR #5) establish a clean, reusable pattern:

```
language-specific AST parser  →  frontend-local IR  →  ModelGraph (shared)  →  expand() → Vec<Triple>  →  ndjson  →  lance-graph SPO store
```

For **C++**, that pattern applies almost unchanged — *except* the AST parser choice carries real cost. The honest evaluation:

- **`clang` crate** (or `clang-sys` lower-level) via libclang is the only mature option with *semantic* understanding (templates, preprocessor, ADL, type resolution). System libclang.so dep is the price; everything that needs to be faithful on real C++ corpora pays it (bindgen, autocxx, cxx-cmake).
- **`tree-sitter-cpp`** is pure-Rust but syntactic-only — insufficient for any predicate that requires resolved types or instantiated templates.
- **`cppast`** is archived. Don't build on it.

Recommended pattern: a new crate `ruff_cpp_spo` mirroring `ruff_ruff_spo` shape:
1. Lock the target triple shape first via a hand-built `ModelGraph` test.
2. Wire `clang` crate as the parser once the shape is locked.
3. `CppClass.declarations: Vec<Declaration>` discriminated union over C++ declaration kinds (methods, ctors, dtors, fields, static methods, template specialisations, virtual overrides, friends, operators).
4. `unpack_declaration` projects into shared `ModelGraph` sibling fields.

**Tesseract is the obvious large reachable corpus**, but the architectural value is the C++ harvester itself, not a Tesseract runtime binding. See §4 — the runtime direction was reverted upstream; the corpus-walk angle is independently useful (LLVM, Boost, OpenCV, …).

---

## 1. The established pattern (recap from PRs #1–#5)

PRs in this repo, in order:
1. **#1** `woa_transcode_harvest` — additive scaffold, uses ruff parser as a library.
2. **#2** `ruff_python_dto_check` — config-driven Python harvester.
3. **#3** `[dto-check] AST→contract→codegen pipeline, lint-calibrated` — the generic AST→contract→codegen engine; `extractors/body.rs` semantic walker, `contract.rs` RouteContract, `codegen/{target,jinja,mod,pipeline}.rs`, `calibrate.rs` five lints.
4. **#4** `feat(spo): language-agnostic SPO triplet expansion + Ruby/Rails scaffold` — factored the SPO core out:
   - `ruff_spo_triplet::{Triple, Predicate, EntityKind, Provenance, ModelGraph, expand, to_ndjson}` — language-agnostic core, serde-only, 15 tests at the time.
   - `ruff_ruby_spo` — Ruby/Rails scaffold; **recommends `lib-ruby-parser` (pure-Rust typed AST)** but defers parser-wiring; locks the target triple shape via a hand-built `WorkPackage` `ModelGraph` test that passes today.
5. **#5** `D-AR-1 + D-AR-2: OpenProject AR-shape predicate vocab + Model IR expansion` — predicates 7 → 34, `Model` grows 12 new `Vec<…>` slots + 1 `Option<StiInfo>`, `RubyClass` swaps `associations + body_source` for `declarations: Vec<Declaration>` (13 variants). 13 new emission arms in `expand()`. `Provenance::OpenProjectExtracted = (0.95, 0.88)`.

The discipline that matters for any new frontend:
- **Lock the shape first.** Hand-build a `ModelGraph` for one representative class, assert `expand()` output, *then* wire the parser. PR #4's `ruff_ruby_spo::tests::locked_shape_expands_to_expected_triples` is the template.
- **Frontend-local discriminated union for declarations.** Don't re-parse source in `extract()`; unpack typed declarations into shared `ModelGraph` slots.
- **`Predicate::ALL` is a closed-vocab gate.** `predicate_count_locked_at_N` test catches drift.
- **Provenance per-predicate-class by default + per-edge overrides** for the principled exceptions (dynamic dispatch ⇒ Inferred; structural-by-construction ⇒ Structural).

---

## 2. C++ parser library evaluation

| Option | Pure-Rust | Semantic resolution | Maturity | Verdict for `ruff_cpp_spo` |
|---|---|---|---|---|
| **`clang` crate** (high-level) / **`clang-sys`** (low-level) — libclang FFI | No (dynamic libclang.so) | **Full** — templates, preprocessor, types, namespaces, ADL | Production (used by bindgen, autocxx) | **The pick.** Same role for C++ that `ruff_python_ast` plays for Python: the canonical AST. System libclang dep is the cost of admission. |
| `bindgen` as substrate | No (uses libclang) | Full but FFI-shaped | Production | Pragmatic shortcut if scope = "harvest Tesseract public surface" rather than every method body. `bindgen --generate ir` gets a structured representation. |
| `autocxx` | No (libclang) | Full | Production | NOT a harvester — binding generator. Mentioned because for *consuming* Tesseract from Rust (the reverted runtime direction) this is the tool. Orthogonal. |
| `tree-sitter-cpp` | **Yes** | **No** — purely syntactic | Mature for editor use | Insufficient for faithful SPO. OK for "list method signatures in a header"; wrong for any predicate that depends on resolved types or template instantiations. |
| `cppast` (Jonathan Müller) | No (libclang wrapper) | Full | **Archived 2022** | Don't build on it. |
| `Boost.Wave` | No (C++) | Preprocessor only | Mature | Narrow tool. Macro-expansion sub-problem only. |
| CHIR / MLIR | No | Full + more | Research-grade | Wrong altitude. |
| Hand-written | Yes | None | N/A | Don't. |

### Recommendation

Use **`clang` crate** as the canonical parser. Set up the crate to detect libclang at build time (the `clang` crate has runtime-load mode + `build.rs` fallbacks). Mirror `ruff_ruby_spo`'s discipline:

```rust
// ruff_cpp_spo (proposed)

pub struct CppClass {
    pub namespace: Vec<String>,
    pub name: String,
    pub declarations: Vec<Declaration>,  // frontend-local discriminated union
}

pub enum Declaration {
    Method(MethodDecl),         // including virtual / override / final
    Constructor(CtorDecl),
    Destructor(DtorDecl),
    Field(FieldDecl),
    StaticMethod(StaticMethodDecl),
    TemplateInstantiation(TemplateInstDecl),
    VirtualOverride(VirtOverrideDecl),
    Friend(FriendDecl),
    Operator(OperatorDecl),
    UsingDecl(UsingDecl),       // using directives + using declarations
    TypeAlias(TypeAliasDecl),
}

pub fn extract(path: &Path) -> ModelGraph { todo!() }   // wire after shape lock
```

Then the `expand()` arms in `ruff_spo_triplet::expand` add C++-flavored predicates (see §3).

---

## 3. C++ predicate vocab — overlap with Ruby, net-new for C++

Overlap with the Ruby/Rails predicates (PR #5):

| Ruby (PR #5) | C++ analog |
|---|---|
| `inherits_from` (STI) | `inherits_from` (single + multiple inheritance; access specifier in object slot) |
| `includes_module` | `using_namespace` (rough analog at the namespace level) |
| `delegates_to` | `delegates_to` (Pimpl idiom; member-of-member calls) |
| `defines_method` | `defines_method` (free fns, methods) |
| `has_attribute` / `has_field` | `has_field` |
| `acts_as` (mixin convention) | `is_friend_of` (intent-disclosing convention) |

Net-new for C++:

| Predicate | What it captures |
|---|---|
| `template_specialises` | Explicit specialisation of a template (partial + full) |
| `template_instantiates` | Materialised instantiation visible in TU |
| `virtually_overrides` | `override` keyword present + virtual base method exists |
| `is_friend_of` | `friend class` / `friend fn` declarations |
| `defines_operator` | Operator overload (per operator-kind in object slot) |
| `uses_macro_expansion` | Identifier originates from macro expansion (preprocessor record) |
| `is_pure_virtual` | `= 0` declaration |
| `is_constexpr` / `is_consteval` | Computed-at-compile-time markers |
| `is_noexcept` | Exception-spec |
| `requires_concept` | C++20 `requires` clause |
| `static_asserts` | `static_assert` in class scope |

For **Tesseract specifically**, additional domain predicates land at frontend or analysis time (not in `ruff_spo_triplet::Predicate` initially):

- `loads_traineddata` (member name matches `LoadModel` / `LoadFromFile` returning a `TessdataManager` / `TFile`)
- `has_recognizer` (composition by `LSTMRecognizer` / `Classify` field)
- `outputs_glyph` (return type of a method matches the glyph type set)
- `consumes_layout_block` (param-type contains `BLOCK` / `BLOCK_LIST`)

These are project-specific and live in a Tesseract analysis layer, not in the closed vocab.

---

## 4. Tesseract as corpus vs Tesseract as runtime — and why the runtime path is reverted upstream

From `AdaWorldAPI/lance-graph` PR #498 body verbatim:

> *"The tesseract-rs cross-repo wiring explored mid-session was **reverted** (board reflects it) — hand-wrapping the original Tesseract C engine is the wrong direction. Pure-Rust OCR via `ocrs` + `rten` (ONNX-adjacent) is the chosen path, parked pending scope."*

Three honest readings of "should `ruff_cpp_spo` proceed":

1. **A C++ SPO harvester is independently useful** (LLVM, Boost, OpenCV, …) and Tesseract happens to be the largest reachable target corpus. **The value is in the harvester, not in a Tesseract runtime binding.** This reading is additive to the substrate.
2. **It's a hedge against `ocrs + rten` not panning out** — keep the C++ understanding-of-Tesseract option warm.
3. **It contradicts the chosen direction.** Don't build it.

Reading 1 is the only one that's clearly *additive*. If you proceed:

- **Tesseract becomes a `ModelGraph` corpus**, not a runtime dep.
- The `AdaWorldAPI/tesseract-rs` fork (default branch `master`) is the natural source of truth for "which Tesseract release to harvest" — pin a commit, run `ruff_cpp_spo` against it, emit ndjson, load into `lance-graph` SPO store.
- The runtime OCR direction stays parked at `ocrs + rten`. No conflict.

---

## 5. Proposed scaffold + sequencing

A `ruff_cpp_spo` crate sized to match `ruff_ruby_spo`:

1. **Shape-lock test (no parser dep).** Build a hand-rolled `Tesseract::Recognizer` `ModelGraph` with 5–10 representative declarations; assert `expand(&graph)` output matches a known set of triples. Mirror `ruff_ruby_spo::tests::locked_shape_expands_to_expected_triples` exactly.
2. **`clang` crate dep + minimal walker.** Walk a single TU (e.g. `tesseract/src/api/baseapi.h`), populate `CppClass.declarations`, run `extract → ModelGraph → expand → triples`, assert output matches the locked shape.
3. **Predicate vocab expansion in `ruff_spo_triplet`.** Add the C++ net-new predicates from §3 (or land them in a follow-up after the corpus-walk run reveals what's actually needed). PR #5's `predicate_count_locked_at_N` gate prevents drift.
4. **`Provenance::CppExtracted`.** Calibrate on Tesseract output (TBD — `(0.92, 0.85)` is a starting guess: lower confidence than `RubyExtracted` because templates + macros add an extra layer of metaprogramming surface).
5. **Walk a Tesseract subset, emit ndjson, load into lance-graph SPO.** First measurable artifact.

Architectural invariants to respect (per the established `ruff_spo_triplet` ethos):

- **Additive.** New crate; touch no other crate. `Cargo.toml` `members = ["crates/*"]` picks it up.
- **Zero new external deps on `ruff_spo_triplet`** — keep it serde-only.
- **`ruff_cpp_spo` depends on `ruff_spo_triplet` + `clang` crate** — the clang dep is the only external addition.
- **No `cargo` invoked in this handover.** Implementation crate; design-spec only.

---

## 6. Open questions for the session that picks this up

1. **Is the corpus-walk angle (Reading 1 of §4) the right framing**, or does the operator prefer to park C++ entirely until `ocrs + rten` proves out?
2. **Which Tesseract release pin** is the canonical corpus — `tesseract-rs` `master` HEAD, or a specific tagged version?
3. **`clang` crate runtime libclang detection** — should the crate build-error if libclang isn't found, or runtime-fail at first `extract()` call?
4. **Cross-link to `lance-graph-codec-research` / `quasicryth-research`** — they have their own KNOWLEDGE.md framings; should `ruff_cpp_spo` cross-cite?
5. **Naming.** `ruff_cpp_spo` matches the `ruff_ruby_spo` / `ruff_python_dto_check` pattern; alternatives could be `ruff_clang_spo` (parser-named) or `ruff_cxx_spo` (idiom-named).

---

## 7. Cross-references

- This repo:
  - `ruff_spo_triplet` (crate) — the language-agnostic core, post-PR #5.
  - `ruff_ruby_spo` (crate) — the Ruby/Rails scaffold, the structural template for `ruff_cpp_spo`.
  - `ruff_python_dto_check` (crate) — the Python harvester + AST→contract→codegen pipeline (PR #3).
  - `ruff_python_dto_check/CODEGEN-DESIGN.md` — design doc for the codegen pipeline.
  - `ruff_spo_triplet/SPO_TRIPLET_EXTRACTION.md` — methodology + Rails-construct → IR mapping table.
- Upstream:
  - `AdaWorldAPI/lance-graph` PR #497 — `Tesseract → tesseract-rs 1:1 transcode v2` plan (the C++ transcode roadmap; v2 corrects v1; layout 1:1 included, LSTM hosted via embedanything → candle → ndarray AMX).
  - `AdaWorldAPI/lance-graph` PR #498 — explicitly reverts the tesseract-rs cross-repo wiring in favour of `ocrs + rten`. **Quote: "hand-wrapping the original Tesseract C engine is the wrong direction."**
  - `AdaWorldAPI/lance-graph` `.claude/plans/tesseract-rs-ast-dll-codegen-v1.md` — the `clang → IR → Rust via ruff` codegen plan.
- Sibling:
  - `AdaWorldAPI/tesseract-rs` `.claude/handovers/2026-06-16-cpp-spo-corpus-handover.md` — the corpus-side companion to this doc.

---

_Authored by an external session (`AdaWorldAPI/bardioc` `session_01VysoWJ6vsyg3wEGc5v7T5v`). Posted under `.claude/handovers/` so the session that owns this repo can pick up with grounded context. No code, no PR — design-spec and evaluation only._
