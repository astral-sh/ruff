# Plan: finish constraint-splitting feature (`lower ≤ T` / `T ≤ upper` as separate individual constraints)

## Handoff status (authoritative)

### Overall
- **Planning/documentation:** ✅ completed
- **Code implementation:** ✅ completed
- **Testing for implementation changes:** ✅ completed

### Phase status
- **Phase A (compile-stabilization):** ✅ completed
- **Phase B (semantic split-constraint migration):** ✅ completed

### Step status (for cross-agent continuation)
- [x] A1. Reintroduce compatibility wrapper `ConstraintSet::constrain_typevar`
- [x] A2. Mechanical compile-fix pass (`constraint.typevar()` + constructor replacements + temporary compatibility views)
- [x] A3. Update implication/intersection/display enough to restore compile with minimal churn
- [x] A4. Reach compile-clean baseline (`cargo check -p ty_python_semantic`)
- [x] B5. Rework `SequentMap` for true split-constraint semantics
- [x] B6. Rework `solutions` / `exists_one` / remaining range assumptions to explicit variant logic

> Note: formatting + full-crate tests are ongoing acceptance criteria and must be rerun after each remaining implementation step.

### Notes on what has been completed so far

> Update: B6 is now complete; any unchecked/todo language in historical sections below should be treated as archival context.
- ✅ Reviewed `constraints.rs` in full
- ✅ Reviewed branch diff vs `main`
- ✅ Identified breakage categories and required migration steps
- ✅ Wrote this handoff plan and clarified that `as_lower_upper` is temporary bridge logic only
- ✅ Reintroduced `ConstraintSet::constrain_typevar(...)` as a compatibility wrapper (`lower_bound(...).and(...upper_bound(...))`)
- ✅ Added temporary compatibility helpers on `Constraint` (`as_lower_upper`, `lower`, `upper`) and mechanically migrated remaining field accesses
- ✅ Replaced all remaining `ConstraintId::new(...)` and `Constraint::new_node(...)` call sites
- ✅ Updated implication/intersection/display and related call paths enough to compile under split constraints
- ✅ Reworked `SequentMap` derivation to operate on split constraints with explicit variant-based propagation
- ✅ Simplified pair sequent derivation into a unified path (same/different typevar cases handled as stanzas in one method)
- ✅ Fixed regressions found by mdtests (notably `generics/*/functions.md` and `type_properties/implies_subtype_of.md`) while keeping the unified structure
- ✅ `cargo fmt --package ty_python_semantic` run
- ✅ `cargo check -p ty_python_semantic` passes
- ✅ `cargo nextest run -p ty_python_semantic` passes (502 passed, 34 skipped)
- ✅ Reworked `exists_one` filtering to explicit split-constraint matching (`LowerBound` / `UpperBound`)
- ✅ Reworked `solutions` path aggregation to explicit split-constraint matching (no compatibility lower/upper view)
- ✅ Updated cross-typevar display-simplification propagation to explicit split-constraint rules
- ✅ Re-ran acceptance checks after B6 (`cargo fmt --package ty_python_semantic`, `cargo nextest run -p ty_python_semantic`)

### Handoff notes for next agent
- In `SequentMap::add_sequents_for_pair`, impossibility for `(L ≤ T) ∧ (T ≤ U)` is intentionally limited to **concrete** `L`/`U` (non-typevar bounds). Making this unconditional regressed `implies_subtype_of` mdtests by over-pruning satisfiable paths.
- `add_relation_propagation_sequents` and `add_concrete_pivot_sequents` are both required:
  - removing explicit relation+relation handling regressed `generics/*/functions.md`
  - removing concrete-pivot handling regressed `type_properties/implies_subtype_of.md`
- Next step B6 should avoid introducing new uses of the temporary compatibility helpers (`as_lower_upper`, `lower`, `upper`) and instead prefer explicit variant matching where feasible.
- After B6, rerun both acceptance criteria from this file (`cargo fmt --package ty_python_semantic` and `cargo nextest run -p ty_python_semantic`).

## Context and current state

The refactor is now in the semantic-migration stage:
- `Constraint` is split (`LowerBound` / `UpperBound`) and compile-stable.
- Constructor and field-access migration is complete.
- `SequentMap` has been migrated to a unified pair-derivation flow with explicit split-constraint rules.
- Full crate tests currently pass at this point in the branch.

## What is currently broken

`cargo check -p ty_python_semantic` now passes.

Remaining work is semantic migration quality, not compile breakage:

1. **Solution/existential logic still contains range-era assumptions** that should be rewritten to explicit variant handling.
2. **After B6, re-run formatting and full crate tests** to reconfirm behavior on the final migration state.

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

(Formatting and full-crate tests are enforced as ongoing acceptance criteria after each remaining step.)

## Validation checklist

- [x] `cargo check -p ty_python_semantic` passes
- [x] no remaining `Constraint::new_node` references
- [x] no remaining `ConstraintId::new(db, builder, ...)` references
- [x] no remaining `constraint.lower/upper/typevar` field accesses on `Constraint`
- [x] external callers compile without forced broad API churn

## Ongoing acceptance criteria (must hold after each remaining step)

- `cargo fmt --package ty_python_semantic`
- `cargo nextest run -p ty_python_semantic`

## Notes / risk areas

- `SequentMap` is the highest-risk area; correctness depends on deriving all needed implications and impossibilities under the new split representation.
- Intersection simplification likely becomes weaker for mixed lower+upper pairs unless explicitly modeled with multi-constraint replacements.
- Keep an eye on source-order stability for derived constraints (existing TODOs already mention this).