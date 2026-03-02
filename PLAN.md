# Plan: finish constraint-splitting feature (`lower ≤ T` / `T ≤ upper` as separate individual constraints)

## Handoff status (authoritative)

### Overall
- **Planning/documentation:** ✅ completed
- **Code implementation:** ⏳ not started
- **Testing for implementation changes:** ⏳ not started

### Phase status
- **Phase A (compile-stabilization):** ⏳ not started
- **Phase B (semantic split-constraint migration):** ⏳ not started

### Step status (for cross-agent continuation)
- [ ] A1. Reintroduce compatibility wrapper `ConstraintSet::constrain_typevar`
- [ ] A2. Mechanical compile-fix pass (`constraint.typevar()` + constructor replacements + temporary compatibility views)
- [ ] A3. Update implication/intersection/display enough to restore compile with minimal churn
- [ ] A4. Reach compile-clean baseline (`cargo check -p ty_python_semantic`)
- [ ] B5. Rework `SequentMap` for true split-constraint semantics
- [ ] B6. Rework `solutions` / `exists_one` / remaining range assumptions to explicit variant logic
- [ ] B7. Run focused tests (`cargo nextest run -p ty_python_semantic`, plus targeted mdtests if needed)
- [ ] B8. Final formatting/lint validation

### Notes on what has been completed so far
- ✅ Reviewed `constraints.rs` in full
- ✅ Reviewed branch diff vs `main`
- ✅ Identified breakage categories and required migration steps
- ✅ Wrote this handoff plan and clarified that `as_lower_upper` is temporary bridge logic only
- ❌ No production code changes made yet

## Context and current state

I reviewed:
- `crates/ty_python_semantic/src/types/constraints.rs` (entire file)
- `jj diff --from main --to @` (current branch delta)

The branch has started the refactor by:
- changing `Constraint` from a range struct to an enum:
  - `Constraint::LowerBound(BoundTypeVarInstance, Type)`
  - `Constraint::UpperBound(BoundTypeVarInstance, Type)`
- adding constructors:
  - `Constraint::new_lower_bound_node(...)`
  - `Constraint::new_upper_bound_node(...)`
- changing some call sites to use the new constructors

But most of `constraints.rs` still assumes range fields (`constraint.lower`, `constraint.upper`, `constraint.typevar`) and old constructors (`Constraint::new_node`, `ConstraintId::new(...)` with 5 args).

## What is currently broken

`cargo check -p ty_python_semantic` currently fails with many errors (89), in two broad categories:

1. **Internal `constraints.rs` logic still range-based**
   - field accesses on enum values (`.lower`, `.upper`, `.typevar`)
   - old constructor calls (`ConstraintId::new(db, builder, ...)`, `Constraint::new_node(...)`)
   - old range-based simplification / sequent code paths

2. **External API breakage from removing `ConstraintSet::constrain_typevar`**
   - callers in `relation.rs`, `signatures.rs`, `call/bind.rs`, and tests still call `ConstraintSet::constrain_typevar(...)`

## Required additional changes

### 1) Reintroduce compatibility entrypoint for range callers

Add back `ConstraintSet::constrain_typevar(db, builder, typevar, lower, upper)` as a convenience wrapper:
- create `lower_bound(...)`
- AND with `upper_bound(...)`
- return combined `ConstraintSet`

This keeps all existing external callers working while internal representation remains split.

### 2) Replace remaining range-field assumptions in `constraints.rs`

Systematically update all remaining uses of:
- `constraint.typevar` -> `constraint.typevar()`
- `constraint.lower` / `constraint.upper` -> variant-aware logic

Likely helper to add on `Constraint` during migration:
- `fn as_lower_upper(self) -> (Type, Type)` with defaults:
  - LowerBound(l) => `(l, object)`
  - UpperBound(u) => `(Never, u)`

This helper is intended as a **temporary compatibility bridge** to get steps 1–3 to a clean compile state, not as the final model.

After the compile-stabilization pass, we should prefer explicit variant logic (`LowerBound` vs `UpperBound`) anywhere polarity matters (especially sequent derivation, implication/intersection simplification, and solution extraction).

### 3) Replace deleted constructor paths

All remaining `ConstraintId::new(db, builder, ...)` calls must become:
- `ConstraintId::new_lower_bound(...)` or
- `ConstraintId::new_upper_bound(...)`

All remaining `Constraint::new_node(...)` calls must be replaced with:
- lower-only or upper-only node creation
- or `lower AND upper` when a range is intended

### 4) Rewrite implication/intersection/display logic for split constraints

The old `ConstraintId::implies` and `ConstraintId::intersect` are range-based.

Need split-aware behavior:
- implication rules by variant pair (`Lower→Lower`, `Upper→Upper`, cross-variant)
- intersection rules by variant pair:
  - same-polarity can simplify to one tighter bound
  - opposite-polarity cannot simplify to one individual constraint; may only detect disjointness
- display logic must render individual constraints directly:
  - lower: `(L ≤ T)`
  - upper: `(T ≤ U)`
  - negations accordingly

### 5) Rewrite sequent-map generation to be split-aware

`SequentMap::{add_sequents_for_single, add_sequents_for_pair, ...}` is currently deeply range-oriented.

Need re-derive rules using individual constraints:
- single-constraint implications:
  - mostly same-polarity implication/tautology checks
- pair-based transitive closure:
  - lower+upper combinations across typevars
  - same-typevar lower+upper interactions that used to live in one range constraint
- any derived “range” postcondition must be emitted as **one or two individual post constraints**
  (never as a single range constraint)

`pair_implications` already supports multiple posts; use that to add both derived bounds.

### 6) Update path-walk and solution extraction code

Sections like:
- `exists_one` mention/bound checks
- `solutions` aggregation (`Bounds::add_lower` / `add_upper`)

still read `constraint.lower/upper` from one node.

Need to switch to variant-based handling:
- LowerBound contributes only lower info
- UpperBound contributes only upper info
- typevar-to-typevar propagation logic remains, but based on which side is present

### 7) Update comment/docs in `constraints.rs`

Top-level and nearby docs still describe an individual constraint as a range.
Update wording to reflect:
- individual constraint is one-sided
- combined range represented by conjunction of two individual constraints

### 8) Keep/adjust builtins/tests that construct ranges

`KnownBoundMethodType::ConstraintSetRange` in `call/bind.rs` can remain range-shaped externally, but must construct via:
- `ConstraintSet::constrain_typevar(...)` compatibility wrapper
  (or explicit `lower_bound(...).and(...upper_bound...)`)

Similarly update/keep tests in `constraints.rs` that currently call `constrain_typevar`.

## Suggested implementation order

**Phase A: compile-stabilization (minimal semantic churn)**
1. Add compatibility wrapper `ConstraintSet::constrain_typevar` back.
2. Mechanical compile-fix pass in `constraints.rs`:
   - replace old field accesses and constructor names.
   - use temporary compatibility views (for example `as_lower_upper`) where needed.
3. Update `ConstraintId::implies`, `ConstraintId::intersect`, and display enough to compile and preserve behavior as closely as possible.
4. Confirm compile-clean baseline with `cargo check -p ty_python_semantic`.

**Phase B: semantic migration to true split-constraint logic**
5. Rework `SequentMap` methods (biggest semantic piece).
6. Rework `solutions` + `exists_one` + any remaining range assumptions, replacing compatibility views with explicit variant logic.
7. Run focused tests:
   - `cargo nextest run -p ty_python_semantic`
   - if needed, specific constraint/mdtests
8. Validate formatting/lints as required by repo workflow.

## Validation checklist

- [ ] `cargo check -p ty_python_semantic` passes
- [ ] no remaining `Constraint::new_node` references
- [ ] no remaining `ConstraintId::new(db, builder, ...)` references
- [ ] no remaining `constraint.lower/upper/typevar` field accesses on `Constraint`
- [ ] external callers compile without forced broad API churn
- [ ] existing constraint display tests pass (or snapshots updated intentionally)
- [ ] mdtests for constraint behavior still pass / are updated for intended semantic changes

## Notes / risk areas

- `SequentMap` is the highest-risk area; correctness depends on deriving all needed implications and impossibilities under the new split representation.
- Intersection simplification likely becomes weaker for mixed lower+upper pairs unless explicitly modeled with multi-constraint replacements.
- Keep an eye on source-order stability for derived constraints (existing TODOs already mention this).