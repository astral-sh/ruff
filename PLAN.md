# Plan: Distinguish missing vs explicit constraint bounds

## Plan maintenance instructions

- Treat this file as the ground truth for ordering, dependencies, and current status.
- Use status markers on every phase/step:
    - `[ ]` not started
    - `[~]` in progress
    - `[x]` complete
    - `[!]` blocked or needs a decision
- When resuming, re-read this plan and inspect the relevant code/tests to validate that the status markers are still accurate before continuing.
- Update this plan whenever implementation details, risks, or completed steps change.

## Goal

Fix <https://github.com/astral-sh/ty/issues/3558>: ty currently represents an absent lower bound as `Never` and an absent upper bound as `object` in individual constraint ranges. This makes an explicit lower bound of `Never` indistinguishable from “no lower-bound information” (and similarly for explicit `object` upper bounds).

The immediate user-visible failures called out by the issue are TODOs where a real `Never` return/lower bound is lost and inference falls back to `Unknown`:

- `crates/ty_python_semantic/resources/mdtest/generics/pep695/functions.md`
    - `ClassWithNoReturnMetatype`
    - `accepts_callable(ClassWithNoReturnMetatype)` should preserve `Never`
- `crates/ty_python_semantic/resources/mdtest/generics/pep695/paramspec.md`
    - `Task(never_returns)` should reveal `Task[(x: int), Never]`
    - calling it should reveal `Never`

## Chosen direction

Use a less-invasive representation than the prior `dcreager/separate-constraints` branch:

- Keep individual constraints as a single range-like structure over one typevar.
- Wrap lower and upper bound fields in `Option<Type<'db>>`.
    - `None` = bound is missing / no information.
    - `Some(Type::Never)` = explicit lower bound of `Never`.
    - `Some(Type::object())` = explicit upper bound of `object`.
- For logical/satisfiability operations, materialize missing bounds as today:
    - effective lower = `lower.unwrap_or(Type::Never)`
    - effective upper = `upper.unwrap_or(Type::object())`
- For solution extraction and inference hooks, preserve bound presence until the caller chooses a solution.

## Important caveats / risks

- Do not use the prior `dcreager/separate-constraints` branch as authoritative. It is known to be in a poor state after merge-conflict resolution removed important heuristics/optimizations.
- Changing only `Constraint { lower, upper }` is insufficient. Presence must survive through `PathBounds`, `TypeVarBounds`, `solve_with`, `PathBounds::default_solve`, and call-site hooks.
- Existing call sites that pass `Type::Never` / `Type::object()` must be audited. Some mean “missing lower/upper bound”; others are explicit bounds and must remain `Some`.
- Tautological constraints may now carry inference evidence. For example, `Some(Never) ≤ T` can be semantically always true while still being important evidence for solving. Be careful with short-circuiting paths such as `is_always_satisfied`, `distributed_or`, and related operations so evidence-bearing tautologies are not accidentally dropped when used for inference.
- Sequent and simplification logic should use materialized effective bounds for logical correctness, but derived constraints should not turn missing bounds into explicit evidence unless the derived fact really provides that evidence.
- The HashMap fallback in `SpecializationBuilder` is still lossy. The Option-backed path should remove the temporary `solve_pending_with` heuristic when constraint-set solving succeeds, but fallback behavior may still need to retain existing safety behavior until it can be removed separately.

## Status

- [x] Read issue #3558 and summarized the intended fix.
- [x] Assessed the Option-backed approach against the current constraint-set code.
- [x] Created this plan.
- [x] Phase 1 implementation is complete.
- [x] Validation for Phase 1 completed: `cargo check -p ty_python_semantic`, `cargo test -p ty_python_semantic types::constraints`, `cargo test -p ty_python_semantic --test mdtest -- type_properties/implies_subtype_of.md`, and `jpk` all passed.
- [x] Phase 2 construction-site audit is complete.
- [x] Phase 4 solving API/caller update is complete, including preserving evidence-bearing tautologies through `or` / `distributed_or` instead of short-circuiting them away.
- [x] Phase 5 target tests are complete: `cargo check -p ty_python_semantic`, `cargo test -p ty_python_semantic types::constraints`, `cargo test -p ty_python_semantic --test mdtest -- type_properties/implies_subtype_of.md`, targeted issue mdtests, and `jpk` all passed.
- [x] Feature scope in this plan is complete.

## Proposed implementation phases

### Phase 1: Introduce explicit bound-presence types

- [x] Add a small representation for materialized bounds-with-presence:
    - `Constraint<'db>` now stores `Option<Type<'db>>` for `lower` and `upper`.
    - `TypeVarBounds<'db>` now stores optional materialized lower/upper bounds.
    - `Bounds<'db>` still accumulates explicit bounds in sets, and empty sets now become `None` during path-bound extraction.
- [x] Provide helper methods for:
    - effective lower (`None` -> `Never`)
    - effective upper (`None` -> `object`)
    - checking whether a lower/upper bound is present
    - constructing constraints from explicit optional bounds internally (`new_with_bounds` / `new_node_with_bounds`)
- [x] Keep public API shape small; existing external callers still use the existing range-style API pending the Phase 2 audit.

### Phase 2: Audit construction sites

- [x] Update `ConstraintSet::constrain_typevar` / `Constraint::new_node` to accept or internally derive bound presence deliberately.
- [x] Add helper constructors where needed so call sites can say whether a `Never`/`object` bound is missing or explicit.
- [x] Audit important current `Type::Never` / `Type::object()` uses in `constraints.rs`, especially:
    - typevar-to-typevar canonicalization
    - `implies_subtype_of`
    - `valid_specializations` / `required_specializations`
    - sequent-derived constraints
    - owned constraint-set loading
- [x] Audit `SpecializationBuilder::add_type_mapping` in `generics.rs`:
    - covariant mapping is explicit lower-only (`Some(ty), None`)
    - contravariant mapping is upper-only (`None, Some(ty)`)
    - invariant mapping remains both explicit (`Some(ty), Some(ty)`)

### Phase 3: Preserve presence during path-bound extraction

- [x] Change `Bounds` so an empty lower set remains `None` rather than becoming `Never` too early.
- [x] Change `Bounds` so an empty upper set remains `None` rather than becoming `object` too early.
- [x] Ensure derived reverse bounds from top-level typevar bounds preserve intended presence.
- [x] Reviewed sorting/stable accumulation behavior; existing source-order path sorting and stable per-bound accumulation still apply, so no further code change was needed.

### Phase 4: Update solving APIs and callers

- [x] Change `PathBounds::solve_with` and `ConstraintSet::solutions_with` to pass lower/upper presence to the solver hook.
- [x] Update variance calculation to use presence, not `lower.is_never()` / `upper == object`.
- [x] Update `PathBounds::default_solve`:
    - explicit `Some(Never)` lower should be a valid selected solution when appropriate
    - missing lower should not force `Never`
    - missing upper should not force `object`
    - declared typevar upper bounds/constraints still validate paths correctly using effective bounds
- [x] Update hook callers in:
    - `generics.rs` (`solve_pending_with`, `build_with` path)
    - `types/call/bind.rs`
    - `types/infer/builder.rs`
- [x] Remove the temporary `solve_pending_with` heuristic once the Option-backed solver preserves explicit `Never`/`object` correctly.

### Phase 5: Tests and snapshots

- [x] Update mdtests for issue #3558 targets:
    - `generics/pep695/functions.md`
    - `generics/pep695/paramspec.md`
- [x] No narrower tests were needed; the target mdtests now cover explicit `Never` lower-bound inference.
- [x] Run targeted mdtests first.
- [x] Review all updated snapshots and any `.pending-snap` files.
- [x] Run `jpk` after changing files. (`jpk` wraps prek in a way that is aware of the jj repo.)

## Testing commands

Use the repository’s standard test environment when running Rust/mdtests:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
CARGO_PROFILE_DEV_DEBUG="line-tables-only" \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo test -p ty_python_semantic --test mdtest -- generics/pep695/functions.md
```

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
CARGO_PROFILE_DEV_DEBUG="line-tables-only" \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo test -p ty_python_semantic --test mdtest -- generics/pep695/paramspec.md
```

After code changes, run the jj-aware pre-commit wrapper:

```sh
jpk
```
