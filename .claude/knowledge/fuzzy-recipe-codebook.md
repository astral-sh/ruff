# The Fuzzy Recipe Codebook — how to cook a `(verb, criteria)` codebook from imperative method bodies

> **Type:** knowledge (methodology — teaches the *how*, not just one answer).
> **READ BY:** any session harvesting method-body facts from ANY AR/OO frontend
>   (`ruff_ruby_spo`, `ruff_python_spo`, `ruff_csharp_spo`, `ruff_cpp_spo`), or
>   designing the OGAR DO-arm (`ActionDef`) lowering. Carried by the
>   `fuzzy-proposer` agent (`.claude/agents/fuzzy-proposer.md`).
> **Status:** FINDING — first cooked + measured 2026-07-06 on the Redmine
>   corpus (OGAR F17 Rails test leg). Method is corpus- and language-agnostic;
>   the numbers are one worked example.
> **Cross-ref:** OGAR `docs/INTEGRATION-MAP.md` F17 row +
>   `E-BODY-TRIAGE-ODOO-CONTROL-1`; op-nexgen
>   `crates/ruff_openproject/tests/body_triage_probe.rs` (the two runnable
>   probes this doc generalizes).

---

## 0. The one-sentence lesson

**An imperative method body is a *fuzzy encoding* of a declarative recipe that
usually already exists in the lifted codebook — so don't transcribe the body,
*correlate* it to its nearest recipe and record only the jitter.**

Transcribing bodies line-by-line is the trap: it reproduces accidental
imperative structure as if it were essential. The win is to recognise that
`self.path = sanitize(self.path)` IS `normalizes :path`, that
`self.x = default if x.blank?` IS a schema default, that `line_ids.update_all`
IS a `dependent:` cascade — recipes the schema/validation/association strata
already carry. What's left after correlation is a small, *named* residue.

## 1. The shape of the problem — `input[shape] {shape × lift × fuzzy} output[shape]`

Every lifecycle method (`before_save`, Odoo `_compute_*`, a C# `OnSaving`, a
C++ setter) maps an `input[record shape]` to an `output[record shape]`. The
**lift** is `body → (verb, criteria)`: the declarative recipe that reproduces
the same shape transform *order-free*. The **fuzzy** is that the body is a
noisy channel — the same recipe shows up spelled many ways. The job is
denoising: recover the recipe, quantify the noise.

This is the CAM-PQ / cascade pattern from the ndarray+lance-graph stack, reused
on code:

| CAM-PQ / cascade term | here |
|---|---|
| vector to encode | a method body |
| fingerprint | the `(W, R, X, C)` fact-set (below) |
| codebook centroid | a canonical recipe (`normalize`/`default`/`compute`/…) |
| nearest-centroid match | recipe correlation |
| residual / jitter | body − nearest recipe (what the recipe can't express) |
| residual palette | the **jitter codebook** (§5) |
| rolling bucket / Belichtungsmesser | re-triage the coarse FAILs through the codebook until the residue is irreducible (§4) |

## 2. The fingerprint — the DTO arm (this is what every frontend must emit)

The fingerprint is FOUR fact sets per method, on `ruff_spo_triplet::Function`:

| field | predicate emitted | provenance | what it captures |
|---|---|---|---|
| `writes` | `writes_field` | **Authoritative** | `self.<f> = …` own-field setters |
| `reads` | `reads_field` | Inferred | own-field reads (incl. condition reads) |
| `raises` | `raises` | Authoritative | `raise X` / `errors.add` abort signals |
| `calls` | `calls` | Inferred | mutator dispatches `"receiver.method"` |

Plus the **visibility split**: hook targets are conventionally *private*, so a
frontend that drops private defs cannot resolve most hooks. `ruff_ruby_spo`
carries them in `Model::helpers` (walked identically, kept out of the routable
action surface). **A frontend without helpers loses ~80% of its hooks to
"no-facts".** (Measured: Redmine went 17/84 → 62/62-resolvable when helpers
landed.)

> **This is the "DTO-arm shape" C# / C++ / Python all still need.** As of
> 2026-07-06 only `ruff_ruby_spo` emits the full quartet + helpers. Coverage:
>
> | frontend | writes | reads | raises | calls | helpers | verdict |
> |---|:-:|:-:|:-:|:-:|:-:|---|
> | `ruff_ruby_spo` | ✅ | ✅ | ✅ | ✅ | ✅ | reference — cook here first |
> | `ruff_python_spo` | ~ | ✅ | ✅ | ~ | ✗ | reads/raises only; **needs writes/calls/helpers** |
> | `ruff_csharp_spo` | ~ | ✗ | ✗ | ✗ | ✗ | **needs the whole arm** (C# `OnSaving`/property setters) |
> | `ruff_cpp_spo` | ~ | ~ | ~ | ~ | ✗ | scaffolded; **needs the arm populated** (setters/virtuals) |
>
> The fingerprint predicates are already in the shared IR
> (`ruff_spo_triplet::Function`) and `expand()` already emits them — a frontend
> "adds the arm" purely by *populating* those Vecs from its AST. Zero IR change.
> Do it per-frontend, then this codebook runs unchanged on that language.

## 3. The recipe codebook — the centroids (pure fact-set predicates, GENERIC)

The centroids are defined ONLY on `(W, R, X, C)` — **no language tokens** — so
the identical codebook classifies Ruby hooks, Odoo `_compute_*`, C# handlers,
C++ methods. First match wins, top to bottom:

```
Compensate  C ∧ X            manual txn (rollback/raise mid-dispatch)  → NO recipe — essential
Cascade     C ∧ ¬X           relation.method dispatch                 → `dependent:` / assoc callback
Guard       X ∧ ¬W           abort-only                               → validation
WriteRaise  W ∧ X            partial-write then escape                → essential (order-dependent)
Compute     W ⊄ R (fresh)    writes a field it did not read           → `emitted_by` compute edge
SelfMap     W ⊆ R            idempotent self-transform                → `normalizes` | schema default
Observe     R only           read-only                                → excluded from the arm
Empty       ∅                no facts                                 → unresolved (scope boundary)
```

**Recoverable** = Compute + SelfMap + Cascade + Guard (order-free recipes).
**Essential** = Compensate + WriteRaise (genuinely order-dependent — keep
imperative; these are the true 15% of the 85/15 split).

## 4. The rolling bucket — win the guessing game

A coarse triage is a *first pass*; it will misfile recoverable recipes into a
FAIL bucket because a coarse predicate can't split them. **Roll the FAILs
through the finer codebook and watch the residue shrink until it's only the
essential kinds.** That convergence IS "won".

Worked example (F17, Redmine, arm 62):

```
Round 0 (coarse triage):   PASS 58 / FAIL 4        (self-feedback 3 + write+raise 1)
Round 1 (recipe codebook): the coarse self-feedback bucket was FUZZY — R∩W
   cannot tell an idempotent SelfMap (order-free) from real accumulation.
   Rolled: FAIL 4 → SelfMap 2 (recovered) + Compute 1 (recovered) + Compensate 1 (essential)
Result:  Cascade 46 · Compute 13 · SelfMap 2 · Compensate 1
         recoverable 61/62 = 98.4% (upper) .. 93.8% (Authoritative-only, Cascade dropped)
         IRREDUCIBLE CORE = 1 Compensate  ← the game is won: no recoverable
                                             recipe left stranded in a FAIL bucket
```

**Win condition (make it a test assertion):** the irreducible core contains
ONLY essential kinds. If a *recoverable* recipe is still hiding in FAIL, you
haven't rolled far enough — refine a centroid or capture a new fact. If the
essential core *grows*, that's a finding (a new order-dependent shape), not
noise — characterize it.

## 5. The jitter codebook — collect the residuals, each names the next fact

Correlation is fuzzy by design; the residuals are not failures, they're the
**map of what one more fact would buy.** Record them as a codebook:

- **J1 — SelfMap degeneracy.** `normalizes` vs schema-default are identical
  under `(W, R)`. Splitting them needs the **guard-predicate fact**
  (`x.blank?` ⇒ default; pure transform ⇒ normalize). Both order-free, so the
  PASS rate is unaffected — only the *emit target* differs. → next fact:
  capture the hook's guard condition.
- **J2 — Cascade rests on Inferred `calls`.** The residual is the
  receiver→`dependent:`-kind codebook (`page.destroy`, `line_ids.update_all`).
  This is why the answer is a **band** (93.8–98.4%), not a point. → next fact:
  resolve the call receiver to a declared association.
- **J3 — composite body.** One hook can be normalize(a,b) + compute(c); the
  recipe is the **set**, not one entry. Order-free if every sub-recipe is. →
  next fact: nothing — just emit a recipe *list* per method.

The jitter codebook is the actionable output: it turns "98.4% recoverable"
into "here are the exact three facts that take it to 100% *targeted*."

## 6. How to cook it (the recipe for the recipe codebook — reproducible)

1. **Pick the reference frontend** with the fullest arm (`ruff_ruby_spo`
   today). Cook here first; port the arm to other frontends after.
2. **Point at a real corpus.** Env-gate + self-skip (ruff #44 house style):
   `RAILS_CORPUS_SRC=/path RAILS_CORPUS_NS=redmine cargo test … -- --nocapture`.
   Never a synthetic fixture for a measurement leg — real bodies or nothing.
3. **PRE-REGISTER thresholds before the first run** (write them in the module
   doc): the pass bar, the KILL floor, the expected tail shapes. The noun-side
   26/26 is *asserted*, so the behaviour side may not borrow it — register its
   own gate. (This is the C5/A-B discipline.)
4. **Coarse triage first, then roll** (§4). Two probes, not one: the coarse
   pass is the honest baseline; the recipe codebook is the refinement. Keep
   both so the delta is visible.
5. **Read the tail bodies in source.** Do NOT infer the tail's shape — open the
   files, confirm each FAIL is what the fingerprint claims (§5 J-notes came
   from reading four real bodies).
6. **Pin the histogram as a drift fuse.** `assert_eq!` the per-bucket counts
   guarded on the corpus signature, so a silent harvest/walker change trips
   loudly. Other corpora print fuse-free.
7. **Record the jitter codebook** as the finding, and file each residual as a
   named next-fact for the frontend.

## 7. Anti-patterns (the ways this goes wrong)

- **Transcribing instead of correlating.** If your output has one Rust fn per
  C# method, you reproduced the fuzz. Correlate to a recipe; emit the recipe.
- **Synthetic corpus for a measurement.** A hand-written fixture proves the
  code runs, never that the *claim* holds. Measurement ⇒ real corpus.
- **Coarse-only, no roll.** Stopping at the first triage over-counts the tail
  (F17 coarse said 6.5% FAIL; the roll showed 1.6% essential). Always roll.
- **Point estimate on Inferred facts.** When a bucket rests on Inferred
  (`calls`), report a *band* (drop it from num+denom for the lower bound), not
  a single number.
- **Silent scope boundary.** Hooks targeting concern/`lib` methods outside the
  harvest scope are "no-facts" — EXCLUDE them, never count them as PASS, and
  print the count so the boundary is visible.
- **Fixing a body "bug" mid-transcode.** Behaviour-preserving: a weird body is
  a finding for an RFC, not a silent fix.

## 8. Why this is the DO-arm's foundation

OGAR's DO arm (`ActionDef` + `KausalSpec`) is *the recipe*, not the body. This
codebook is how a producer frontend decides, per method, whether a body lowers
to a declarative recipe (85%: `normalizes`/default/compute/cascade/guard →
`ActionDef`) or must stay a hand-ported imperative core (15%:
compensate/write-raise → raw method). Cook the codebook per language, and each
consumer collapses to "a compiler-store caller + a small essential residue."
That is the 85/15 split, measured rather than asserted.
