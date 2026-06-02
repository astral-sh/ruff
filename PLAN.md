# Plan: deferred quantification for callable constraint-set typevars

## Procedural notes

- Status markers:
    - `[ ]` not started
    - `[~]` in progress
    - `[x]` complete
    - `[?]` open question / needs confirmation
- Keep this file as the ground-truth handoff document. When resuming, validate that status markers still match the code before continuing.
- Use `jj` for all source-control operations. Do not edit an existing revision with `jj edit`; create a new revision with `jj new` / `jj new -A @` before making changes. Prefix revision descriptions with `[π]`.
- Use `$HOME/.pi/tmp` rather than `/tmp` for temporary files.
- For ty work, follow `.agents/skills/working-on-ty/SKILL.md` and the repo `AGENTS.md` instructions.
- All code changes must be tested. If snapshots are updated, review them. Run `uvx prek run --files <changed files>` at the end of implementation work.

## Current status

- `[x]` Initial planning revision created: `[π] Plan deferred quantification for callable constraints`.
- `[x]` Relevant code paths were surveyed and the plan was reviewed for implementation.
- `[x]` Phase 1 implementation revision created: `[π] Add deferred constraint-set quantification metadata`.
- `[x]` Phase 1 is implemented: `ConstraintSet` and `OwnedConstraintSet` now carry `deferred_quantification`, default constructors use `InferableTypeVars::None`, helper methods exist for recording/applying/merging deferred quantification, owned/query/load round trips preserve the metadata, and a focused Rust unit test covers owned/query/load metadata preservation.
- `[ ]` Next step: Phase 2 should propagate deferred metadata through constraint-set operations and switch public semantic observations to apply deferred quantification while keeping construction short-circuiting raw.

## Background and goal

When checking assignability between callable types, generic callable-local type variables are treated as inferable for the duration of the signature comparison. Today `TypeRelationChecker::check_signature_pair` in `crates/ty_python_semantic/src/types/signatures.rs` calls `ConstraintSet::reduce_inferable` before returning, existentially quantifying those signature-local type variables immediately.

That immediate reduction hides the freshened callable type variables from later constraint-set construction. We want to defer that existential quantification so later work can still add constraints involving the freshened variables (for example, constraints that tie a generic callable argument to another argument in a higher-order call). The near-term goal is still to quantify these variables away before solution extraction, preserving the existing solver assumptions.

## Relevant existing code

- `crates/ty_python_semantic/src/types/signatures.rs`
    - `TypeRelationChecker::check_signature_pair`:
        - freshens source/target signatures when needed;
        - computes `source_inferable` and `target_inferable` from the (possibly freshened) signatures;
        - checks the signature pair with those variables merged into the relation's `inferable` set;
        - currently calls `when.reduce_inferable(db, self.constraints, source_inferable.iter(db).chain(target_inferable.iter(db)))` before returning.
    - `CallableSignature::when_constraint_set_assignable_to` and signature overload handling return these constraint sets to callers.
- `crates/ty_python_semantic/src/types/constraints.rs`
    - `ConstraintSet` contains a raw `NodeId`, its builder reference, and `deferred_quantification: InferableTypeVars<'db>` metadata. The metadata is represented and preserved, but most operations still need Phase 2/4 updates to apply or propagate it.
    - `OwnedConstraintSet` stores the owned arenas for cached/interened constraint sets plus the deferred-quantification metadata.
    - `ConstraintSet::reduce_inferable` still performs immediate existential abstraction via `NodeId::exists` / `InteriorNode::exists_one`.
    - `ConstraintSet::solutions`, `solutions_with`, `remove_noninferable`, and `Type::assignable_solutions_with_inferable` are still the main solution-generation/projection paths and have not yet been refactored for deferred quantification.
    - `PathBounds::compute` currently takes a raw `NodeId`, so any deferred metadata must be applied before calling it.
- `crates/ty_python_semantic/src/types/generics.rs`
    - `SpecializationBuilder::solve_pending_with` removes non-inferable constraints and then solves pending constraints.
    - `SpecializationBuilder::add_type_mappings_from_constraint_set` removes non-inferable constraints and extracts solutions from a constraint set returned by callable-signature inference.
- `crates/ty_python_semantic/src/types/relation.rs`
    - `Type::when_constraint_set_assignable_to_owned` caches an `OwnedConstraintSet` and must preserve any deferred-quantification metadata.

## Design invariants

- The raw constraint-set node should continue to mention freshened callable type variables after signature checking returns.
- The set should also carry metadata saying which type variables must be existentially quantified before final solution extraction / final semantic observation.
- Conjunction/intersection with later constraints must be able to see and constrain those variables before they are quantified.
- Solution generation must apply deferred existential quantification before `remove_noninferable` and before `PathBounds::compute`, matching the current effective solver input.
- Owned/cached constraint sets must preserve deferred-quantification metadata across `into_owned`, `query`, and `load`.
- Boolean/satisfiability observations need an explicit policy: final user-visible relation checks should preserve today's semantics, but internal combinator short-circuiting must not eagerly erase raw constraints that future construction could use.

## Resolved design decisions

- `[x]` Core semantic model: a deferred-quantification `ConstraintSet` is a raw constraint formula plus a set of pending existential variables. While building larger positive formulas, the raw formula remains intact so later constraints can mention those variables. Before final semantic observation or solution extraction, the effective formula is `∃ deferred_vars. raw_formula`.
- `[x]` Deferred quantification records exactly the same type-variable identities that `TypeRelationChecker::check_signature_pair` currently passes to `ConstraintSet::reduce_inferable`: `source_inferable ∪ target_inferable`, computed after the current freshening logic. This feature does not change which variables are quantified, only how and when they are quantified.
- `[x]` Representation: store deferred-quantification variables directly as `InferableTypeVars<'db>` on `ConstraintSet` and `OwnedConstraintSet`, using field/helper names that refer to deferred quantification. Do not rename `InferableTypeVars` in this PR; minimizing the diff is more important. During implementation, add a TODO near the `InferableTypeVars` definition noting that it now has a dual purpose and might eventually be renamed to something like `InternedTypeVarSet`.
- `[x]` Positive combinators merge deferred metadata globally: `(P, D) ∧ (Q, E)` becomes `(P ∧ Q, D ∪ E)`, and `(P, D) ∨ (Q, E)` becomes `(P ∨ Q, D ∪ E)`. `Node` and related BDD types remain the raw representation and do not know about deferred quantification. Construction short-circuiting in `ConstraintSet::and` / `or` should use raw `Node` saturation checks directly, while public `ConstraintSet::is_always_satisfied` / `is_never_satisfied` should first apply deferred quantification and therefore report semantic/effective satisfiability. Concretely, public semantic checks call `apply_deferred_quantification`; construction methods inspect `self.node` directly. If raw short-circuiting skips a thunk, do not force it just to collect deferred metadata; any skipped quantifiers are vacuous relative to raw `false ∧ _` or raw `true ∨ _`, and preserving thunk laziness is important for performance. Add a comment documenting this. `IteratorConstraintsExtension::when_all` / `when_any` must keep using `NodeId::distributed_and` / `distributed_or` for performance and accumulate deferred metadata alongside the distributed node construction.
- `[x]` Solution-generation layering: avoid requiring callers to manually call `remove_noninferable` before `solutions`. Refactor `ConstraintSet::solutions` / `solutions_with` / `path_bounds` so callers choose whether to project to a caller-inferable set or solve all currently present typevars. The implementation always applies deferred quantification first, then optionally removes non-inferable constraints, then computes/solves paths. This is the key behavior change of the PR: even `SolutionProjection::AllTypeVars` applies deferred quantification; it only skips the later non-inferable projection. `PathBounds` and `Node` remain lower-level/raw and do not carry deferred-quantification metadata. Use an explicit `constraints::SolutionProjection` enum with variants like `AllTypeVars` and `InferableOnly(InferableTypeVars<'db>)` so “skip non-inferable projection” cannot be confused with “project using `InferableTypeVars::None`”. Add `ConstraintSet::path_bounds(...)` as the public/module-facing wrapper, make `PathBounds::compute` module-private, and make `ConstraintSet::remove_noninferable` private or remove that wrapper entirely so callers cannot bypass the required ordering. Implement `ConstraintSet::solutions` / `solutions_with` via `ConstraintSet::path_bounds(...).solve(...)` / `.solve_with(...)` so `path_bounds` is the single projection/quantification gateway. After updating callers, remove `NodeId::solutions_with` if unused; otherwise keep it private as a raw helper.
- `[x]` `ConstraintSet::reduce_inferable` should be replaced, not preserved as another public-ish operation. Its only current caller is the callable/signature reduction site. Replace that call with `ConstraintSet::with_deferred_quantification(...)`, and keep eager existential abstraction as an internal `ConstraintSet` helper that applies stored deferred quantification by calling raw `Node`/`NodeId` existential-abstraction machinery and clearing the metadata. `with_deferred_quantification` must accept an `InferableTypeVars<'db>` (not an iterator) and merge with any existing deferred metadata rather than overwrite it.
- `[x]` Operations that introduce negation semantics force deferred quantification first. For this PR, `negate`, `implies`, `iff`, and `implies_subtype_of` should operate on the effective formula after applying stored deferred quantification, then clear deferred metadata. This preserves current behavior. `ConstraintSet::negate` returns a set with empty deferred metadata because the quantified variables no longer exist in the effective raw BDD. `ConstraintSet::implies` should preserve RHS thunk laziness: force the left side before negation, but only evaluate and force the RHS if the implication's OR construction needs it. `ConstraintSet::iff` takes its RHS eagerly, so force both sides before constructing the raw equivalence and return empty deferred metadata. `ConstraintSet::implies_subtype_of` should apply deferred quantification to `self` before delegating to the raw implication machinery; the newly created subtype constraint has no deferred metadata of its own. The more invasive future design would track quantifier structure/polarity through negation; document this limitation with TODO comments near these operations.
- `[x]` `ConstraintSet::satisfied_by_all_typevars` is a public/terminal semantic observation and should apply deferred quantification first. This method is currently only used in tests/debug `ty_extensions.ConstraintSet` flows; applying deferred quantification best mimics current behavior. If it becomes part of real inference later, revisit if needed.
- `[x]` Owned/cached constraint sets preserve deferred metadata exactly across `ConstraintSetBuilder::into_owned`, `OwnedConstraintSet::query`, and `ConstraintSetBuilder::load`. Store/copy the `InferableTypeVars<'db>` value directly; no builder-local remapping is needed because it contains stable `BoundTypeVarIdentity<'db>` values. `OwnedConstraintSet::default` uses `InferableTypeVars::None`. Do not intern deferred metadata in `ConstraintSetBuilder`'s typevar arena; the builder arena remains for raw constraints/type occurrences. `OwnedConstraintSet` derives (`Eq`, `Hash`, `salsa::Update`, etc.) should include the new field so Salsa/cache identity reflects deferred metadata.
- `[x]` Optimize for the common case where deferred metadata is `InferableTypeVars::None`. Applying deferred quantification should return quickly without BDD work when metadata is empty. This will make public semantic methods fast in the common case, since they can call that helper first. This works well with `InferableTypeVars`'s existing efficient empty enum variant and `merge` fast paths. The private helper should be named `apply_deferred_quantification` and return the effective raw `NodeId`, making the boundary from public `ConstraintSet` semantics to raw BDD operations explicit. This helper only applies the stored deferred metadata; solution projection with `SolutionProjection` is a separate step layered on top.
- `[x]` `ConstraintSet::verify_builder` only needs to verify the builder pointer. Deferred metadata stores stable `BoundTypeVarIdentity<'db>` values via `InferableTypeVars<'db>` and is intentionally not builder-local, so no additional builder verification is needed.
- `[x]` `ConstraintSet`'s custom `Debug` impl should include `deferred_quantification` but continue to omit `builder`. The important invariant of the custom impl is avoiding noisy/non-useful builder output while still showing semantically relevant metadata.
- `[x]` No special recursion/cycle-cache changes are needed. Relation visitors cache/cloned `ConstraintSet` values, and the new deferred metadata field will be included naturally as long as `ConstraintSet` remains `Copy`/`Clone`.
- `[x]` `ConstraintSet` must remain `Copy`; `InferableTypeVars<'db>` is already `Copy`, so the new field should not change that.
- `[x]` Detailed `ConstraintSet` display should expose deferred quantification explicitly as a boolean formula, e.g. `∃T, U . P`, where `P` is the raw constraint formula. `ConstraintSet::display` itself should render this quantifier prefix whenever deferred metadata is non-empty; otherwise it behaves as it does today. Use the raw node display for `P` (including the existing display simplification), not the effective node after applying deferred quantification. Display all recorded deferred variables, even if some are vacuous after simplification. This keeps the debugging value of seeing what callable checking recorded and what the BDD structure looks like, while presenting the set's effective semantics as an existential formula. Non-detailed truthiness/display paths still use semantic/effective satisfiability. Do not add tests that assert or otherwise exercise exact display text; constraint-set rendering is for debugging and is intentionally not stable across ty processes.
- `[x]` Terminology: use “deferred quantification” consistently in the plan, code identifiers, and comments. Code-facing names should use `deferred`, such as `deferred_quantification`, `with_deferred_quantification`, and `apply_deferred_quantification`.
- `[x]` Documentation/comments: update module/struct docs in `constraints.rs` to describe the raw `Node`/TDD layer plus deferred existential metadata on `ConstraintSet`. Document that positive construction preserves/merges metadata, public semantic observations and solution extraction apply it first, and negation-like operations force it because quantifier structure through negation is future work. Add short operation-local TODO comments near `negate`, `implies`, `iff`, and `implies_subtype_of`, and update the `check_signature_pair` comment to explain that signature-local variables are still existentially quantified but now recorded on the returned `ConstraintSet` instead of reduced immediately.
- `[x]` Keep no-builder public methods as no-builder methods. `ConstraintSet::is_always_satisfied`, `is_never_satisfied`, and `display` should continue to use `self.builder`; do not add a builder parameter just to verify it. Methods that already accept a builder parameter should continue to call `verify_builder`.
- `[x]` `InternedConstraintSet::with_detailed_display` needs no special changes. `InternedConstraintSet` continues to store an `OwnedConstraintSet` plus the `detailed_display` flag; deferred metadata is preserved by `OwnedConstraintSet`.
- `[x]` Constraint-set truthiness needs no special changes beyond preserving metadata in `load` and making public `ConstraintSet::is_always_satisfied` semantic/effective. The existing truthiness path calls `is_always_satisfied`, so it will automatically apply deferred quantification.
- `[x]` Implementation should proceed in phases, with each phase held in its own jj revision as much as practical. Each revision should be clean: compile, pass the relevant targeted tests, and have required formatting/linting/pre-commit checks run for the files changed. Testing is listed as Phase 6 below, but in practice tests should be added/run piecemeal with the phase that introduces each behavior.

## Proposed implementation outline

Each phase should generally become its own jj revision if it can be made clean independently. The phases are a conceptual overview, not a strict dependency ordering: implementation may tackle sub-steps from later phases before every sub-step of an earlier phase is complete if that produces cleaner changes. Do not treat Phase 6 as a final-only testing phase; add and run tests incrementally with the implementation phase that needs them.

### Phase 1: Represent deferred quantification

- `[x]` Add deferred-quantification metadata to constraint sets.
    - Add an `InferableTypeVars<'db>` field (for example `deferred_quantification`) to `ConstraintSet<'db, 'c>` and `OwnedConstraintSet<'db>`.
    - Keep field/helper names in terms of deferred quantification, even though the storage type is `InferableTypeVars<'db>`.
    - This keeps `ConstraintSet` `Copy` and reuses existing ordered identity storage/merge/iteration for `BoundTypeVarIdentity<'db>`.
    - Add a TODO near the `InferableTypeVars` definition explaining that it now also represents deferred existential variables, so we might eventually rename it (for example to `InternedTypeVarSet`). Do not perform that rename in this PR.
- `[x]` Add constructors/helpers such as:
    - keep `ConstraintSet::from_node`, `from_bool`, and `constrain_typevar` defaulting to `InferableTypeVars::None` for deferred metadata,
    - an internal constructor/helper for building a `ConstraintSet` from a raw `NodeId` plus explicit deferred metadata,
    - `ConstraintSet::with_deferred_quantification(...)` (accepts an `InferableTypeVars<'db>`, records those typevars without changing the node, and merges with any existing deferred metadata),
    - an internal helper named `apply_deferred_quantification` that applies stored deferred quantification via raw `Node`/`NodeId` existential abstraction and returns the effective raw `NodeId`; it must return quickly without BDD work when metadata is `InferableTypeVars::None`,
    - a helper to merge metadata when combining two sets.
- `[x]` Update `OwnedConstraintSet::default`, `OwnedConstraintSet::query`, `ConstraintSetBuilder::into_owned`, and `ConstraintSetBuilder::load` to preserve the metadata by copying the `InferableTypeVars<'db>` value directly. No builder-local remapping is needed.

### Phase 2: Propagate metadata through operations

- `[ ]` Ensure raw-preserving positive combinators merge deferred metadata globally:
    - `and` / `intersect`: `(P, D) ∧ (Q, E)` becomes `(P ∧ Q, D ∪ E)`; `intersect` mutates/returns `self` with merged deferred metadata and should verify both operands' builders.
    - `or` / `union`: `(P, D) ∨ (Q, E)` becomes `(P ∨ Q, D ∪ E)`; `union` mutates/returns `self` with merged deferred metadata and should verify both operands' builders.
    - `IteratorConstraintsExtension::when_all` / `when_any` must keep using `NodeId::distributed_and` / `distributed_or` for performance and accumulate/merge metadata from every generated constraint set alongside the distributed node construction.
    - Construction short-circuiting in `ConstraintSet::and` / `or` should call raw `Node` `is_never_satisfied` / `is_always_satisfied` directly. Do not use public `ConstraintSet` semantic satisfiability methods for construction short-circuiting. If the RHS thunk is skipped, do not force it to collect deferred metadata; add a comment that skipped quantifiers are vacuous and preserving thunk laziness is intentional for performance.
- `[ ]` Operations that do not commute with existential quantification force deferred quantification before proceeding:
    - `negate`: apply deferred quantification before negating, so `¬(∃T. P)` is preserved instead of accidentally becoming `∃T. ¬P`.
    - `implies`: apply deferred quantification to the left side before negation; preserve RHS thunk laziness, and if the RHS is evaluated, apply deferred quantification to it before combining.
    - `iff`: apply deferred quantification to both operands before building the operation.
    - `implies_subtype_of`: apply deferred quantification to `self` before delegating to raw implication logic; the newly created subtype constraint is raw and carries no deferred metadata.
    - Add TODO comments explaining that a future, more invasive design could track quantifier structure/polarity through these operations; that is explicitly out of scope for this PR.
- `[ ]` Raw vs effective satisfiability API policy:
    - `Node` and related BDD types remain the raw representation; their existing `is_always_satisfied` / `is_never_satisfied` checks are raw checks because they cannot see deferred quantification metadata.
    - `ConstraintSet::and` / `or` should use raw `Node` checks for construction short-circuiting, so effective truth after existential quantification does not erase useful raw constraints.
    - Public `ConstraintSet::is_always_satisfied`, `ConstraintSet::is_never_satisfied`, and `satisfied_by_all_typevars` should apply deferred quantification first and therefore report semantic/effective satisfiability.

### Phase 3: Defer the callable/signature reduction

- `[ ]` In `TypeRelationChecker::check_signature_pair`, replace the final immediate `reduce_inferable` call with `with_deferred_quantification`, recording the same identities as deferred quantification:
    - compute `signature_inferable = source_inferable.merge(db, target_inferable)`;
    - use `self.inferable.merge(db, signature_inferable)` for relation checking;
    - pass `signature_inferable` to `with_deferred_quantification` after the inner check. The current code shadows the `inferable` variable, so rename variables to keep the signature-local set available.
- `[ ]` Keep the relation checker itself using the merged inferable set while checking the signature pair. Only the final projection is deferred.
- `[ ]` Update comments explaining that signature-local generic variables are still existentially quantified, but that quantification is recorded on the returned `ConstraintSet` instead of applied immediately. Explain that this keeps freshened callable variables visible to later positive constraint construction, while semantic observation / solution extraction still applies the quantification.

### Phase 4: Apply deferred quantification at solution generation

- `[ ]` Refactor solution-generation APIs so callers no longer manually pre-project constraint sets. The API must distinguish two modes:
    - solve all currently present typevars without `remove_noninferable` projection;
    - project to a caller-provided inferable set before solving.
- `[ ]` Add `ConstraintSet::path_bounds(db, builder, projection)` to centralize deferred quantification and solution projection before calling `PathBounds::compute`. Make `PathBounds::compute` module-private so callers cannot bypass that ordering accidentally. Implement `ConstraintSet::solutions` and `solutions_with` through `path_bounds(...).solve(...)` and `path_bounds(...).solve_with(...)`.
- `[ ]` Add a small explicit enum for that mode: `SolutionProjection::AllTypeVars` vs `SolutionProjection::InferableOnly(InferableTypeVars<'db>)`. Derive at least `Clone`, `Copy`, and `Debug`; add Salsa/hash/size traits only if compiler errors require them. Do not use `Option<InferableTypeVars<'db>>`; `None` would be too easy to confuse with `InferableTypeVars::None`.
- `[ ]` Each `ConstraintSet` solution/path-bounds API must enforce this order internally:
    1. call `apply_deferred_quantification`, which only existentially abstracts the metadata stored on the `ConstraintSet`; this happens for every `SolutionProjection`, including `AllTypeVars`;
    1. if requested, remove non-inferable constraints for the caller's inferable set according to `SolutionProjection`;
    1. solve / compute `PathBounds`.
- `[ ]` Update current manual projection callers:
    - `SpecializationBuilder::solve_pending_with`
    - `SpecializationBuilder::add_type_mappings_from_constraint_set`
    - `Type::assignable_solutions_with_inferable`
- `[ ]` Make `ConstraintSet::remove_noninferable` private or remove that wrapper entirely after updating callers. The centralized `solutions` / `solutions_with` / `path_bounds` methods should be the only public/module-facing way to perform solution projection.
- `[ ]` Audit nested solution helpers such as `PathBounds::default_solve`, which call `when_constraint_set_assignable_to_owned(...).query(... is_never_satisfied ...)`, to ensure deferred metadata is applied for semantic checks during solution extraction.

### Phase 5: Audit terminal observations and debugging APIs

- `[ ]` Update module/struct docs in `constraints.rs` to explain the deferred-quantification model and the boundary between public `ConstraintSet` semantics and raw `Node`/TDD operations.
- `[ ]` Add short operation-local TODO comments near `negate`, `implies`, `iff`, and `implies_subtype_of`, explaining that forcing deferred quantification preserves current behavior because `∃` does not commute with negation, and that preserving quantifier structure belongs to future work.
- `[ ]` Audit all `is_always_satisfied` / `is_never_satisfied` callers that can receive constraint sets involving callables.
    - Final relation checks in `relation.rs` should preserve today's behavior.
    - Error-context and overload-pruning checks should use semantic/effective satisfiability unless there is a clear reason to inspect the raw node.
- `[ ]` Update `ConstraintSet::display` to render deferred quantification as a boolean formula prefix, e.g. `∃T, U . P`, where `P` is the raw constraint formula using the existing display simplification. Do not apply deferred quantification before displaying `P`. Display all recorded deferred variables, even if some are vacuous after simplification. Non-detailed display/truthiness should continue to use semantic/effective satisfiability. Do not add tests that assert or otherwise exercise exact display text.
- `[ ]` Ensure `KnownInstanceType::ConstraintSet` display and boolean truthiness continue to behave sensibly when metadata is present.

### Phase 6: Tests

- `[ ]` Add focused Rust unit tests in `constraints.rs` for deferred-quantification metadata:
    - semantic satisfiability matches eager existential quantification for final observations;
    - positive conjunction preserves raw constraints and applies existential reduction at solving, e.g. `(T = int)` with deferred `{T}` conjoined with `U = T` can solve `U = int` after quantifying `T`;
    - owned round-trip preserves metadata through `into_owned` / `query` / `load`; because this is a Rust unit test, directly collect the metadata field into an `FxHashSet` and assert equivalence before and after, in addition to any semantic check;
    - negation/implies behavior follows the chosen compatibility semantics, including a case where `¬(∃T. P)` differs from `∃T. ¬P`;
    - `SolutionProjection::AllTypeVars` vs `SolutionProjection::InferableOnly(...)` behave differently as intended.
    - Do not add tests that assert or otherwise exercise exact constraint-set display text.
- `[ ]` Do not add new mdtests unless implementation uncovers a missing user-visible regression case. Existing mdtests already cover the relevant callable/generic/ParamSpec behavior.
- `[ ]` Update existing mdtests and TODO comments if behavior improves. In particular, this work is expected to resolve the “Multiple occurrences of a higher-order generic callable” TODOs in:
    - `crates/ty_python_semantic/resources/mdtest/generics/legacy/callables.md`
    - `crates/ty_python_semantic/resources/mdtest/generics/pep695/callables.md`
- `[ ]` Existing mdtests to run/review include:
    - `generics/legacy/callables.md`
    - `generics/pep695/callables.md`
    - generic callable assigned/passed cases where existential specialization should succeed;
    - generic callable comparisons that should still fail;
    - overloaded callable cases if snapshots change;
    - ParamSpec cases, because callable/signature logic has special ParamSpec paths.
- `[ ]` Run targeted tests first:
    - `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo test -p ty_python_semantic`
    - or narrower mdtest commands while iterating.
- `[ ]` Review all updated snapshots / pending snapshots.
- `[ ]` Run `uvx prek run --files <changed files>` before declaring implementation complete.

## Open questions for plan review

- `[x]` Should deferred quantification be represented directly as `InferableTypeVars<'db>`, or should we introduce a distinct `DeferredQuantification` type for clarity even if it wraps the same ordered set of `BoundTypeVarIdentity<'db>`? Resolved: use `InferableTypeVars<'db>` directly, with explicit deferred-quantification field/helper names and a TODO about possible future renaming.
- `[x]` Should public `ConstraintSet::is_always_satisfied` / `is_never_satisfied` apply deferred quantification automatically, while internal combinators use raw helpers? Resolved: public `ConstraintSet` methods apply deferred quantification and report semantic/effective satisfiability. `Node` methods remain raw, and `ConstraintSet::and` / `or` use raw `Node` checks directly for construction short-circuiting.
- `[x]` For `negate` / `implies` / `iff`, is forcing quantification before the operation acceptable? Resolved: yes. Also force before `implies_subtype_of`. Add TODO comments that preserving quantifier structure through negation belongs to a separate, more invasive future design.
- `[x]` Should detailed `ConstraintSet` display expose deferred quantification explicitly (for debugging), or should it render the effective quantified result? Resolved: expose it explicitly as a formula prefix, e.g. `∃T, U . P`, while non-detailed truthiness/display remains semantic/effective.
