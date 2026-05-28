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
- [ ] No implementation has started yet.

## Proposed implementation phases

### Phase 1: Introduce explicit bound-presence types

- [ ] Add a small representation for materialized bounds-with-presence, likely by changing:
    - `Constraint<'db> { lower: Type<'db>, upper: Type<'db> }`
    - `Bounds<'db>` accumulation
    - `TypeVarBounds<'db>`
- [ ] Provide helper methods for:
    - effective lower (`None` -> `Never`)
    - effective upper (`None` -> `object`)
    - checking whether a lower/upper bound is present
    - constructing explicit lower-only, upper-only, and full range constraints
- [ ] Keep public API shape small; prefer local helper methods over broad refactors.

### Phase 2: Audit construction sites

- [ ] Update `ConstraintSet::constrain_typevar` / `Constraint::new_node` to accept or internally derive bound presence deliberately.
- [ ] Add helper constructors where needed so call sites can say whether a `Never`/`object` bound is missing or explicit.
- [ ] Audit important current `Type::Never` / `Type::object()` uses in `constraints.rs`, especially:
    - typevar-to-typevar canonicalization
    - `implies_subtype_of`
    - `valid_specializations` / `required_specializations`
    - sequent-derived constraints
    - owned constraint-set loading
- [ ] Audit `SpecializationBuilder::add_type_mapping` in `generics.rs`:
    - covariant mapping should probably be explicit lower-only (`Some(ty), None`)
    - contravariant mapping should probably be upper-only (`None, Some(ty)`)
    - invariant mapping remains both explicit (`Some(ty), Some(ty)`)

### Phase 3: Preserve presence during path-bound extraction

- [ ] Change `Bounds` so an empty lower set remains `None` rather than becoming `Never` too early.
- [ ] Change `Bounds` so an empty upper set remains `None` rather than becoming `object` too early.
- [ ] Ensure derived reverse bounds from top-level typevar bounds preserve intended presence.
- [ ] Update sorting/stable accumulation behavior without regressing deterministic output.

### Phase 4: Update solving APIs and callers

- [ ] Change `PathBounds::solve_with` and `ConstraintSet::solutions_with` to pass lower/upper presence to the solver hook.
- [ ] Update variance calculation to use presence, not `lower.is_never()` / `upper == object`.
- [ ] Update `PathBounds::default_solve`:
    - explicit `Some(Never)` lower should be a valid selected solution when appropriate
    - missing lower should not force `Never`
    - missing upper should not force `object`
    - declared typevar upper bounds/constraints still validate paths correctly using effective bounds
- [ ] Update hook callers in:
    - `generics.rs` (`solve_pending_with`, `build_with` path)
    - `types/call/bind.rs`
    - `types/infer/builder.rs`
- [ ] Remove the temporary `solve_pending_with` heuristic only once the Option-backed solver preserves explicit `Never`/`object` correctly.

### Phase 5: Tests and snapshots

- [ ] Update mdtests for issue #3558 targets:
    - `generics/pep695/functions.md`
    - `generics/pep695/paramspec.md`
- [ ] Add narrower tests if needed for explicit `Some(Never)` lower vs missing lower.
- [ ] Run targeted mdtests first.
- [ ] Review all updated snapshots and any `.pending-snap` files.
- [ ] Run `uvx prek run --files <changed files>` for every changed file.

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

After code changes, run pre-commit hooks only over changed files, for example:

```sh
uvx prek run --files PLAN.md <other changed files>
```
