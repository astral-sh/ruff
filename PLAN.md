# Plan: Migrate SpecializationBuilder from type_mappings HashMap to ConstraintSet

## Status: In progress (Phases 1–4 complete; Phase 5.1–5.4 complete)

## Overview

The `SpecializationBuilder` (in `crates/ty_python_semantic/src/types/generics.rs`) currently lives
in a hybrid state:

- **Old solver**: Most match arms in `infer_map_impl` walk formal/actual types manually, find
    typevar assignments, and add them to a `FxHashMap<BoundTypeVarIdentity, Type>` (`self.types`).
    When a typevar gets multiple assignments, the old solver combines them via union. (This is correct
    when the typevar assignment appears in a contravariant position. If the assignment appears in
    covariant position, we should use intersection!)

- **New solver**: A couple of match arms (callables and protocols) create a `ConstraintSet` via the
    constraint set machinery, then immediately extract solutions and add them back to the old
    `type_mappings` hash map via `add_type_mappings_from_constraint_set`.

The "hybrid" part is that for the types that use the new solver, we immediately extract solutions
from the constraint, and then add them to the type_mappings list that the old solver builds up.

The goal is to change the builder's internal "pending" state from the `type_mappings` hash map to a
`ConstraintSet`, bringing us one step closer to using the new solver everywhere.

## Instructions for agents

The proposed plan has detailed steps organized into phases. Steps within a phase and across
phases form a DAG, not a strict linear sequence — the dependency graph is documented below so
that you can always identify the frontier of available work. Status markers indicate which steps
and phases have already been completed. When resuming a plan that a previous agent created, read
through other files in the repo as necessary to validate that the status markers are accurate.

## Three-pattern framework

Every call site that reaches into the builder's pending state falls into one of three patterns:

- **Pattern 1 (constraint conjunction)**: The logic can be expressed as a constraint set itself,
    which should be conjoined (AND'd) into the builder's pending constraint set.

- **Pattern 2 (solution extraction hook)**: We provide a new hook that is given the lower/upper
    bounds of a typevar in a solution, which controls which particular type in that range is chosen.

- **Pattern 3 (standalone constraint set query)**: The call site uses a *temporary*
    `SpecializationBuilder` purely to query per-typevar information — it never calls `build_with()`
    to produce the final specialization (or only builds an intermediate one for downstream use).
    In the new world, these cases **bypass `SpecializationBuilder` entirely**, typically via
    `Type::assignable_solutions(_with_inferable)` plus `PathBounds::{solve, solve_with}`.

Patterns 1 and 2 apply to the call sites that actually *build a specialization*. Pattern 3
applies to temporary builders that are just used as query mechanisms. The distinction matters
because Pattern 3 sites don't need any `SpecializationBuilder` API changes — they exit the
builder's domain entirely.

## Architectural shifts

Two major changes happen naturally as part of this migration:

### `infer_reverse` goes away

`infer_reverse` exists because the old solver's type-walk logic (in `infer`) was written to only
support comparisons in one direction. The new solver uses `when_constraint_set_assignable_to`,
which correctly handles inferable typevars in either the lhs or rhs.

The `ConstraintSetAssignability` relation (in `relation.rs` around line 450) creates a constraint
when a typevar appears on *either side* of the comparison:

- Typevar on LHS (`T ≤ target`): creates constraint `Never ≤ T ≤ target`
- Typevar on RHS (`source ≤ T`): creates constraint `source ≤ T ≤ object`

This contrasts with the non-CSA path, which returns `true` (losing info) for an inferable LHS
typevar (line ~795) and `false` for an inferable RHS typevar (line ~1051).

### The `f` callback in `infer_map` goes away

The `f` callback lets callers filter/modify type mappings before they're added. In the new world,
either the filtering is expressed as constraints (Category 1), or the modification happens at
solution extraction time (Category 2).

## Call site analysis

### 1. `preferred_type_mappings` + `infer_argument_constraints` callback (`call/bind.rs`)

Pattern: 1 (constraint conjunction)

**Builds a specialization**: Yes — this is the main specialization for a generic function call.

**Current behavior**: The preferred-type query already uses forward CSA, but only as a
*standalone query*:

1. `return_ty.assignable_solutions_with_inferable(...)` computes cached per-path bounds for
    `return_ty ≤ tcx`
1. `PathBounds::solve_with(...)` default-solves those bounds while also recording per-typevar
    variance from the bounds themselves
1. The resulting preferred types are filtered (covariant positions, top-level inferable-typevar
    artifacts, unspecialized typevars, and “no concrete content” results), then seeded into the
    builder via `insert_type_mapping`
1. `infer_argument_constraints` still uses the `infer_map` callback to suppress argument
    assignments that are already compatible with the preferred type, and to flip
    `assignable_to_declared_type` when a non-partially-specialized preferred type conflicts with an
    argument inference

So this site is already using CSA for the TCX query, but it still collapses the TCX constraint
set into per-typevar preferred types *before* argument inference starts.

**New approach**: Conjoin the raw TCX constraint set with the argument constraints inside the
builder. “Is the argument inference compatible with the preference?” becomes “is the combined
constraint set satisfiable?”. The retry logic becomes: try with TCX constraints conjoined; if the
combined set is unsatisfiable, drop them and retry.

**Worked examples**:

- `f[T](x: T) -> list[T]` with `result: list[int] = f(True)`: TCX gives `T ≤ int`, arg gives
    `T ≥ bool`, combined `bool ≤ T ≤ int`, satisfiable. Solution hook picks upper bound for
    non-covariant → `T = int`. Same as current.
- `f[T](x: T) -> list[T]` with `result: list[int] = f("hello")`: TCX gives `T ≤ int`, arg
    gives `T ≥ str`, combined unsatisfiable (`str` not `≤ int`). Retry without TCX → `T = str`.
    Same as current.

**Subtlety: covariant filter.** Today this filter is driven by the variance reported from the
TCX query's path bounds. Without the filter, we'd keep additional upper-bound-only preferences
that are usually harmless, but can cause spurious unsatisfiability and unnecessary retries.

**Subtlety: `partially_specialized_declared_type`.** The current code still tracks whether
TCX-derived types contain unspecialized typevars, and softens the “not assignable” error for
those. In the fully-conjoined world, we'd need a cleaner way to keep that heuristic (or eliminate
it entirely).

**Eliminates**: the remaining `preferred_type_mappings` map, the `infer_map` callback, and the
`assignable_to_declared_type` bookkeeping. (`infer_reverse_map` is already gone.)

### 2. `maybe_promote` via `build_with` (`call/bind.rs`)

Pattern: 2 (solution extraction hook)

**Builds a specialization**: Yes — same builder as #1, this is the specialization-construction
step.

**Current behavior**: After inference, `build_with` is called. Because the builder is still
HashMap-backed, the hook currently sees *synthetic equality bounds* `(mapped_ty, mapped_ty)` for
mapped typevars only. `maybe_promote` checks the typevar's variance in the return type and its
declared bounds, and may promote literals (e.g. `Literal[1] → int`).

**New approach**: Once the builder's pending state becomes a `ConstraintSet`, the same hook
should run against the real per-path lower/upper bounds coming out of the pending set. If the
lower bound is a literal type, the typevar appears in a non-covariant position in the return
type, and the promoted type still fits inside the upper bound, choose the promoted type.

**Feasibility**: Straightforward.

**Already eliminated**: `mapped()`.

### 3. Bidirectional argument inference (`infer/builder.rs`)

Pattern: 3 (standalone constraint set query)

**Builds a specialization**: Technically yes, but only as an intermediate — it creates a partial
specialization to apply to parameter types for downstream bidirectional inference. It is not the
main specialization of the call.

**Current behavior**: This migration is complete. The code now:

1. `return_ty.assignable_solutions(db, declared_return_ty)`
1. solves the cached `PathBounds` via `PathBounds::solve(...)`
1. builds `tcx_mappings` from the resulting solutions
1. creates the intermediate specialization with `GenericContext::specialize_recursive`, using
    `UnspecializedTypeVar` for unsolved typevars

No `SpecializationBuilder` is involved anymore.

**Feasibility**: Done.

### 4. `infer_reverse_map_impl`'s internal builder (`generics.rs`)

Pattern: Goes away entirely (internal implementation detail of `infer_reverse`)

**Builds a specialization**: No — purely internal to the reverse inference mechanism.

**Current behavior**: This machinery is gone. It used to create a temporary builder with
synthetic typevars, call `infer` in the forward direction, extract the map, and then use those
mappings to map synthetic typevars back to actual typevars while recursing into reverse
inference.

**Why it goes away**: This whole mechanism existed because the old solver could only do forward
inference. The CSA relation handles the recursive specialization walk naturally. For
`Container[Box[T]] ≤ Container[Box[int]]`, the relation walks `Container`'s specialization, then
`Box`'s, producing `T ≤ int` (or `T = int` for invariant containers). No synthetic typevars are
needed.

### 5. Historical `types.rs` site (`visit_specialization_impl`)

Pattern: 3 (standalone constraint set query)

**Status**: Complete, and no current `types.rs` caller remains.

This item is kept only as historical context: it was one of the original `infer_reverse` callers
that justified Phase 3, but the surrounding code has since been refactored away.

### 6. `infer_collection_literal` (`infer/builder.rs`)

This code path now has **one standalone query plus one builder finalization step**.

#### 6a. TCX query (`infer/builder.rs`)

Pattern: 3 (standalone constraint set query)

**Builds a specialization**: No — it extracts per-typevar TCX constraints
(`elt_tcx_constraints`) and per-typevar variance (`elt_tcx_variance`) for downstream use.

**Current behavior**: This migration is complete. The code now:

1. `collection_instance.assignable_solutions_with_inferable(...)`
1. solves the cached `PathBounds` via `solve_with(...)`
1. records per-typevar variance from the bounds themselves (`lower = Never` ⇒ covariant,
    `upper = object` ⇒ contravariant, otherwise invariant), joining across paths
1. filters out inferable-typevar artifacts and unspecialized-typevar contamination from the solved
    types
1. retains variance entries only for typevars that still have surviving TCX constraints

This bound-derived variance replaced an earlier plan to recover variance from the collection type
structure; the bound-based approach turned out to be both simpler and sufficient.

**Remaining relevance to later phases**: This query is already in its intended standalone form.
The only follow-on work is in the *second* builder that consumes its output.

#### 6b. Element inference + singleton-promotion hook (`infer/builder.rs`)

Pattern: 1 (constraint conjunction) **plus** 2 (solution extraction hook)

**Builds a specialization**: Yes — this is the final specialization for the collection type.

**Current behavior**: The second builder now:

- injects surviving non-covariant TCX constraints via `insert_type_mapping`
- adds per-element constraints via `infer(...)`
- finishes with `build_with(...)`, using a hook that promotes singleton lower bounds to
    `T | Unknown` when there were no TCX constraints (so e.g. `[None]` becomes
    `list[None | Unknown]`)

**New approach**: When Phase 5 switches the builder's internal representation to a
`ConstraintSet`, this site should keep the same high-level shape. The important requirement is
that the existing singleton-promotion hook continue to look at the *lower* bound, now sourced
from the pending constraint set instead of synthetic equality bounds.

## PathBounds and extraction hooks

The solution-extraction refactor has since settled into two surfaces:

- **Standalone `Type ≤ Type` queries** use cached `PathBounds`, via
    `Type::assignable_solutions(...)` and `Type::assignable_solutions_with_inferable(...)`
- **Builder-internal constraint sets** still use `ConstraintSet::solutions_with(...)`

`PathBounds::default_solve(...)` is the default “pick a representative type from these bounds”
policy used by both paths.

Current hook signatures:

```rust
// standalone query / raw-constraint-set hooks
FnMut(
    BoundTypeVarInstance<'db>,
    TypeVarVariance,
    Type<'db>,
    Type<'db>,
) -> Result<Option<Type<'db>>, ()>

// SpecializationBuilder::build_with
FnMut(
    BoundTypeVarInstance<'db>,
    Type<'db>,
    Type<'db>,
) -> Option<Type<'db>>
```

Notes:

- `Ok(None)` means “fall back to `PathBounds::default_solve` for this path”
- `Err(())` invalidates the current path
- `SpecializationBuilder::build_with` still has the simpler hook shape because the builder is
    HashMap-backed today; it only exposes synthetic lower/upper bounds for already-mapped
    typevars

**Multi-path BDD handling**: Keep the current per-path behavior. Run the hook per path, then
combine the chosen per-path results via union. This matches the current hybrid behavior of
`add_type_mappings_from_constraint_set`.

## Feasibility summary

| Call site                                | Pattern   | Feasible?  | Risk/Concern                                                   |
| ---------------------------------------- | --------- | ---------- | -------------------------------------------------------------- |
| `preferred_type_mappings` + callback     | 1         | Yes        | `partially_specialized_declared_type` still needs a clean exit |
| `maybe_promote` via `build_with`         | 2         | Yes        | Must keep working when `build_with` sees real bounds           |
| Bidirectional argument inference         | 3         | Done       | Already uses cached `PathBounds`                               |
| `infer_reverse_map_impl` internal        | Goes away | Done       | —                                                              |
| Historical `types.rs` site               | 3         | Done       | No current caller remains                                      |
| `infer_collection_literal` TCX query     | 3         | Done       | Variance now comes from path bounds                            |
| `infer_collection_literal` final builder | 1 + 2     | Mostly yes | Preserve singleton-promotion hook across the Phase 5 switch    |

No fundamental blockers. Main design challenges *for the remaining work*:

1. Switching `build_with` from synthetic equality bounds to real per-path bounds from
    `self.pending`
1. Preserving or eliminating the `partially_specialized_declared_type` heuristic cleanly
1. Behavioral differences from HashMap union vs constraint conjunction (especially invariant /
    contravariant cases)

## Completeness verification

The current public `SpecializationBuilder` API is much smaller than when this plan was first
written:

| Method                | Current callers                                        | Plan coverage         |
| --------------------- | ------------------------------------------------------ | --------------------- |
| `new`                 | `call/bind.rs:3917,4060`; `infer/builder.rs:6239`      | Call sites #1 and #6  |
| `build_with`          | `call/bind.rs:4121`; `infer/builder.rs:6363`           | Call sites #2 and #6b |
| `insert_type_mapping` | `call/bind.rs:4037`; `infer/builder.rs:6268`; internal | Call sites #1 and #6b |
| `infer`               | `infer/builder.rs:6297,6300,6346`                      | Call site #6b         |
| `infer_map`           | `call/bind.rs:4146`                                    | Call site #1          |

Removed APIs such as `mapped`, `with_default`, `infer_reverse`, `infer_reverse_map`,
`into_type_mappings`, and the old `build()` entry point are already covered by completed
Phases 1–4.

All former `infer_reverse` / `infer_reverse_map` callers remain accounted for. The surviving
standalone-query sites now use the cached `PathBounds` helpers:

- **Bidirectional argument inference** (`infer/builder.rs`): uses
    `return_ty.assignable_solutions(...)`, then `PathBounds::solve(...)`
- **`infer_specialization` preferred types** (`call/bind.rs`): uses
    `return_ty.assignable_solutions_with_inferable(...)`, then `solve_with(...)`; variance and
    filtering are driven by the solved path bounds
- **`infer_collection_literal` TCX query** (`infer/builder.rs`): uses
    `collection_instance.assignable_solutions_with_inferable(...)`, then `solve_with(...)`;
    variance is likewise derived from the bounds, not from `variance_of(...)`
- **Historical `types.rs` site**: no current caller remains, but the migration is complete

## Key file locations

- **`SpecializationBuilder`**: `crates/ty_python_semantic/src/types/generics.rs`
- **Standalone query helpers / `PathBounds` / `default_solve`**:
    `crates/ty_python_semantic/src/types/constraints.rs`
- **Builder-internal `ConstraintSet::solutions_with`**:
    `crates/ty_python_semantic/src/types/constraints.rs`
- **CSA typevar handling**: `crates/ty_python_semantic/src/types/relation.rs`
- **`infer_specialization`** (call sites #1, #2):
    `crates/ty_python_semantic/src/types/call/bind.rs`
- **`infer_argument_constraints`**: same file
- **Bidirectional argument inference** (call site #3):
    `crates/ty_python_semantic/src/types/infer/builder.rs`
- **`infer_collection_literal`** (call site #6): same file

## Validation

After each step, run:

```sh
cargo nextest run -p ty_python_semantic --cargo-profile fast-test
```

For steps that might have broader impact (especially Phase 5 steps), also run:

```sh
cargo nextest run --cargo-profile fast-test
```

```sh
cargo nextest run -p ty_python_semantic -p ty_ide --cargo-profile fast-test
```

For deeper validation (especially Phases 5–6), run ecosystem analyses using the
`local-ecosystem` skill on `aiortc`, `sympy`, `static-frame`, and `vision`, and compare
diagnostics against a `main` baseline to ensure no regressions.

If tests fail due to behavioral changes from CSA (usually more precise types), **do not update
test expectations without confirming with @dcreager first**. Document which tests changed and
why, so that the semantics-impacting changes can be reviewed for legitimacy. Use `cargo insta accept` for snapshot tests only after confirmation.

Line numbers in this plan are approximate and may drift as the codebase evolves. Agents should
use `rg` to find the current locations of the key identifiers (`SpecializationBuilder`,
`infer_reverse`, `preferred_type_mappings`, `add_type_mappings_from_constraint_set`, etc.)
rather than relying on line numbers.

## Phased plan

### Dependency graph

```text
Phase 1 (hook API design)
  │
  ├──► Phase 2 (Pattern 2: solution extraction hooks)
  │      │
  │      └──► Phase 4.2 (preferred_type_mappings)──► Phase 4.4 (remove infer_reverse)
  │                                                        │
  Phase 3 (Pattern 3: standalone CS queries) ──────────────┤
  │                                                        │
  │    Phase 4.1 (conjoin method) ──► Phase 4.3 (coll. literal conjunction)
  │                                                        │
  │                                                        ▼
  │                                              Phase 5 (switch internal repr
  │                                                        + eliminate f callback)
  │                                                        │
  └────────────────────────────────────────────────────────►│
                                                      ┌────┴────┐
                                                      ▼         ▼
                                            Phase 6          Phase 7
                                     (migrate infer_map   (replace preferred_
                                      _impl arms;         type_mappings with
                                      optional cleanup)   CS conjunction)
```

Key observations:

- **Phase 1** and **Phase 3** have no dependencies and can start immediately, in parallel.
- **Phase 2** depends on Phase 1 (needs the hook API).
- **Phase 4** steps have mixed dependencies: Step 4.1 is independent; Step 4.2 depends on
    Phase 2 (same builder) and Phase 3 (eliminates remaining `infer_reverse` callers);
    Step 4.3 depends on Steps 3.2 and 4.1; Step 4.4 depends on all other Phase 3/4 steps.
- **Phase 5** depends on Phase 4 (callers must be migrated before we change the internal repr).
    Phase 5 switches the builder to a `ConstraintSet` AND eliminates the `f` callback (there is
    only one non-trivial caller, and it can be restructured to use satisfiability checks instead).
- **Phase 6** and **Phase 7** both depend on Phase 5, but are independent of each other.
    Phase 6 is optional cleanup — the `infer_map_impl` arms work correctly when their output
    flows through constraints. Phase 7 replaces the `preferred_type_mappings` mechanism with
    direct constraint set conjunction.

### Phase 1: Design the solution extraction hook API

Status: Complete ✅
**Difficulty: Medium** — requires design decisions about multi-path BDD handling and the hook
signature, but the implementation is modest (refactoring existing code in `solutions()`).
**Dependencies: None** — can start immediately.

The solution-extraction refactor has since settled into two related surfaces:
cached standalone `PathBounds` queries for `Type ≤ Type` checks, and
`ConstraintSet::solutions_with(...)` for builder-internal constraint sets.

**Step 1.1 ✅**: Refactored the old `solutions()` logic into reusable building blocks:

- `Bounds`: accumulates raw lower/upper bounds per typevar
- `TypeVarBounds`: materialized lower/upper bounds (union of lowers, intersection of uppers)
- `PathBounds::compute()`: computes sorted BDD paths and materializes per-typevar bounds
- `PathBounds::default_solve()`: default solution selection for a single typevar on one path
- `PathBounds::solve_with()`: applies a per-typevar solver across all paths

**Step 1.2 ✅**: Designed and implemented the hook surface used today:

- `ConstraintSet::solutions_with(...)` for builder-internal constraint sets
- `Type::assignable_solutions(...)` / `assignable_solutions_with_inferable(...)` for cached
    standalone queries
- Hook signature at the constraint/path-bounds layer:
    `FnMut(BoundTypeVarInstance, TypeVarVariance, Type, Type) -> Result<Option<Type>, ()>`
    - Receives the typevar, its path-local variance, and materialized lower/upper bounds
    - `Ok(Some(ty))` overrides the default solution
    - `Ok(None)` falls back to `PathBounds::default_solve`
    - `Err(())` invalidates the current path
- Later follow-up cleanups removed the old generic `Solutions<S>` wrapper and the builder-local
    solution cache; standalone queries now cache `PathBounds` instead

**Step 1.3 ✅**: Implemented `build_with` on `SpecializationBuilder` as the
specialization-construction entry point. It is still HashMap-backed today:

- the hook is called only for mapped typevars
- mapped typevars are exposed as synthetic equality bounds `(mapped_ty, mapped_ty)`
- unsolved typevars are left as `None` so `specialize_recursive` can fill in defaults
- there is no separate `build()` method anymore; all current callers use `build_with`

### Phase 2: Migrate Pattern 2 call sites (solution extraction hooks)

Status: Complete ✅
**Difficulty: Easy–Medium** — the `maybe_promote` migration is mostly mechanical once the hook
API exists.
**Dependencies: Phase 1** (needs the `build_with` API).

**Step 2.1 ✅**: Migrated `maybe_promote` (`call/bind.rs`). Replaced
`builder.mapped(generic_context, maybe_promote).build(generic_context)` with
`builder.build_with(generic_context, maybe_promote)`, where the hook returns `Some(promoted)`
to override or `None` to keep the default. The hook closure captures `self` for access to the
return type, call expression TCX, and typevar bound/constraint info.

Also fixed `build_with` to only call the hook for *mapped* typevars (those with entries in the
type mappings). Unmapped typevars are passed through as `None` to `specialize_recursive` so they
get filled in with defaults. The original implementation called the hook for all typevars
including unmapped ones (with synthetic `Never`/`object` bounds), which caused hooks like
`maybe_promote` to produce `Some(Never)` for unmapped typevars instead of leaving them as `None`.

**Step 2.2 ✅**: Removed `mapped()` from `SpecializationBuilder`'s public API.

### Phase 3: Migrate Pattern 3 call sites (standalone constraint set queries)

Status: Complete ✅
**Difficulty: Easy–Medium per step** — each is self-contained. Step 3.2 is the hardest due to
variance tracking.
**Dependencies: None** — can start immediately, in parallel with Phase 1.

These sites no longer use temporary `SpecializationBuilder` instances. The preferred
standalone-query surface is now `Type::assignable_solutions(_with_inferable)` plus
`PathBounds::{solve, solve_with}`.

These were good early migration targets because they were self-contained — changing them didn't
affect the `SpecializationBuilder` API or its remaining callers.

**Step 3.1 ✅**: Historical `types.rs` site. This migration was completed, and the surrounding
code path has since disappeared from the current call graph. Keep this step as historical context
only.

**Step 3.3 ✅**: Migrated bidirectional argument inference (`infer/builder.rs`):

- Replaced `infer_reverse(declared_return_ty, return_ty)` with the cached standalone query
    `return_ty.assignable_solutions(db, declared_return_ty)`
- Solved via `PathBounds::solve(db, &constraints)` and built `tcx_mappings` from the resulting
    solutions
- Created the intermediate specialization via `GenericContext::specialize_recursive`, using
    `Some(mapped_ty)` for solved typevars and
    `Some(Type::Dynamic(DynamicType::UnspecializedTypeVar))` for unsolved ones
- Removed the temporary `SpecializationBuilder` and `with_default` call

**Step 3.2 ✅**: Migrated the TCX query in `infer_collection_literal` (`infer/builder.rs`):

- Replaced `infer_reverse_map(tcx, collection_instance, ...)` with the cached standalone query
    `collection_instance.assignable_solutions_with_inferable(db, tcx, inferable)`
- Solved via `PathBounds::solve_with(...)`
- **Variance approach**: determine variance from the solved bounds rather than from the collection
    type structure:
    - `lower = Never` (no lower bound) → covariant
    - `upper = object` (no upper bound) → contravariant
    - both bounds set → invariant
        This correctly handles cases where the TCX type is a covariant superclass of the collection
        (e.g. `Sequence[Any]` as TCX for `list[T]`)
- Applied partially-specialized-typevar filtering post-hoc on solutions
- **Key invariant**: retain variance entries only for typevars that still have actual TCX
    constraints after filtering (`elt_tcx_variance.retain(...)`)
- Removed the first temporary `SpecializationBuilder`
- **Cross-typevar filtering**: filtered inferable typevars out of solved unions so that
    SequentMap-derived cross-typevar relationships do not leak into the preferred concrete type
- Later follow-up cleanups removed the temporary `OwnedConstraintSet::query` /
    `solutions_with_inferable` layer; the code now goes straight through cached `PathBounds`
- **Test results**: The `ty_python_semantic` crate tests passed at the time of this step. One test expectation was updated:
    - `literal_promotion.md:220`: `list[Y[Literal[1]]]` → `list[list[Literal[1]]]` — type
        alias `Y` (defined as `type Y[T] = list[T]`) is resolved because the CSA normalizes the
        annotation before creating constraints. Semantically equivalent; confirmed acceptable.

### Phase 4: Migrate Pattern 1 call sites (constraint conjunction) and finish eliminating `infer_reverse`

Status: Complete ✅
**Difficulty: Hard** — Step 4.2 is the most complex migration in the entire plan, touching the
core specialization inference logic with subtle heuristics (`partially_specialized_declared_type`,
covariant filtering, retry logic).
**Dependencies:**

- Step 4.1 has no dependencies (can start any time).
- Step 4.2 depends on Phase 2 (same builder uses `build_with`) and Phase 3 (all other
    `infer_reverse` callers must be migrated first, so we can validate removal).
- Step 4.3 depends on Steps 3.2 (TCX query migrated) and 4.1 (`insert_type_mapping` exists).
- Step 4.4 depends on Steps 3.1–3.3, 4.2, 4.3 (all `infer_reverse` callers migrated).

**Step 4.1** ✅: Added `insert_type_mapping` to `SpecializationBuilder`. A temporary
`conjoin_constraint_set` helper was also added during the migration, but later removed once the
code settled on the cached `assignable_solutions*` helpers for standalone queries.

**Step 4.2** ✅: Migrate the `preferred_type_mappings` pattern in `infer_specialization`
(`call/bind.rs`):

Replaced `infer_reverse_map(tcx, return_ty, ...)` with the cached forward-CSA query
`return_ty.assignable_solutions_with_inferable(...)`.

Implementation details and follow-up cleanups:

- **CSA handler** (`relation.rs`): No semantic change — CSA still constrains all typevars, and
    inferability filtering happens after the query is built
- The standalone query path now materializes cached `PathBounds` directly; earlier transitional
    helpers such as `OwnedConstraintSet::query` and `solutions_with_inferable` have since been
    removed
- `assignable_solutions_with_inferable(...)` handles inferability filtering internally; a later
    cleanup to `remove_noninferable` kept mixed bounds on inferable typevars even when those
    bounds mention non-inferable typevars
- Preferred-type variance is derived from path bounds via `solve_with(...)`, not from the type
    structure
- **Preferred type filtering** (`bind.rs`) still applies three solution-level filters:
    1. Remove top-level inferable typevars (SequentMap transitivity artifacts)
    1. Remove types with unspecialized typevars (partially specialized contexts)
    1. Skip solutions where no union element is purely concrete (no typevars at any depth)

Behavioral change (improvement): `annotations.md:611` — the old code could not infer preferred
types through union TCXs (e.g. `list[Any] | None`). The CSA approach correctly infers `T = Any`
from the annotation, changing the revealed type from `list[int] | None` to `list[Any] | None`.
Test expectation updated.

**Step 4.3** ✅: Simplified the TCX injection in `infer_collection_literal`
(`infer/builder.rs`):

- Replaced `builder.infer(Type::TypeVar(elt_ty), elt_tcx)` with
    `builder.insert_type_mapping(elt_ty, elt_tcx)`, which directly inserts the type mapping
    without going through `infer_map_impl`
- The covariant filtering and Unknown fallback logic remain unchanged
- Note: the original plan envisioned conjoining the raw CSA constraint set here. That is still
    deferred to Phase 5 because this code path needs both (a) covariant-typevar filtering and
    (b) an explicit Unknown fallback when there is no applicable TCX constraint

**Step 4.4** ✅: Removed all dead code from the old reverse inference machinery:

- Removed `infer_reverse`, `infer_reverse_map`, `infer_reverse_map_impl` from
    `SpecializationBuilder`.
- Removed `into_type_mappings` (only caller was inside `infer_reverse_map_impl`).
- Removed `with_default` (had `#[expect(dead_code)]`; last caller migrated in Phase 3 Step 3.3).
- Removed `UniqueSpecialization` variant from `TypeMapping` enum and all associated match arms
    across `types.rs`, `generics.rs`, `known_instance.rs`, and `typevar.rs`.
- Removed the `UniqueSpecialization`-specific branch in `Specialization::apply_type_mapping_impl`
    that created synthetic type variables.
- Cleaned up unused imports: `ruff_python_ast::name::Name` (generics.rs),
    `std::cell::RefCell` (types.rs).

### Phase 5: Switch internal representation to ConstraintSet

Status: In progress (Steps 5.1–5.4 complete)
**Difficulty: Medium–Hard** — the mechanical changes are straightforward, but behavioral
differences in how constraints combine (vs HashMap union) may cause test changes.
**Dependencies: Phase 4** (callers must be migrated so that `infer_reverse` is gone).

#### Rationale for doing this before migrating `infer_map_impl` arms

The original plan had Phase 5 (migrate `infer_map_impl` arms to CSA) before Phase 6 (switch
internal repr). This ordering is unnecessary and undesirable:

1. **Each `add_type_mapping` call already has all the information needed to create a constraint**:
    the typevar, the inferred type, and the polarity (which maps directly to lower/upper bounds).
    We don't need to rewrite the type-walk logic to produce constraints — we can convert at the
    `add_type_mapping` boundary.

1. **The new-solver arms (callable/protocol) can AND their local constraint sets directly**,
    eliminating the `add_type_mappings_from_constraint_set` stopgap (which extracts solutions
    only to re-insert them into the HashMap). This is the TODO that the code itself calls out.

1. **Migrating `infer_map_impl` arms is optional cleanup** that can happen incrementally later.
    Once the builder maintains a constraint set, each arm's `add_type_mapping` calls produce
    constraints that flow through the solver naturally. The arms still do useful work (type
    structure walking, bound/constraint checking, error detection).

#### How `add_type_mapping` maps to constraints

Each call to `add_type_mapping(typevar, ty, polarity, f)` translates to:

- **Covariant** polarity: `ty` is a lower bound on the typevar.
    Constraint: `constrain_typevar(T, ty, object)` i.e. `ty ≤ T ≤ object`.
- **Contravariant** polarity: `ty` is an upper bound on the typevar.
    Constraint: `constrain_typevar(T, Never, ty)` i.e. `Never ≤ T ≤ ty`.
- **Invariant** polarity: `ty` is both a lower and upper bound (equality).
    Constraint: `constrain_typevar(T, ty, ty)` i.e. `T = ty`.
- **Bivariant** polarity: no constraint (the typevar is unconstrained by this position).

The constraint is AND'd into the builder's pending `ConstraintSet`.

#### Behavioral differences from HashMap union

The HashMap combines multiple assignments via union regardless of polarity. The constraint set
combines them via AND, which produces different results depending on polarity:

| Polarity      | HashMap behavior                    | Constraint set behavior                                                               | Different?             |
| ------------- | ----------------------------------- | ------------------------------------------------------------------------------------- | ---------------------- |
| Covariant     | `T = ty1 \| ty2` (union)            | `T ≥ ty1` AND `T ≥ ty2` → `T ≥ ty1 \| ty2`, solution picks lower bound → `ty1 \| ty2` | **Same**               |
| Contravariant | `T = ty1 \| ty2` (union, **wrong**) | `T ≤ ty1` AND `T ≤ ty2` → `T ≤ ty1 & ty2`, solution picks upper bound → `ty1 & ty2`   | **Yes — more correct** |
| Invariant     | `T = ty1 \| ty2` (union, **wrong**) | `T = ty1` AND `T = ty2` → unsatisfiable if `ty1 ≠ ty2`                                | **Yes — more correct** |

The covariant case (the most common) is unchanged. The contravariant and invariant cases are
improvements, but may cause test changes that need review.

**Invariant case detail**: Two equality constraints `T = ty1` AND `T = ty2` where `ty1 ≠ ty2`
would be unsatisfiable. The current code unions them to `T = ty1 | ty2`, which is incorrect
(e.g., for `dict[T, T]` called with `{1: "hello"}`, the current code infers `T = int | str`,
but `dict[int, str]` is NOT assignable to `dict[int | str, int | str]` since dict is invariant).
The constraint set approach correctly detects the inconsistency. Test changes from this are
expected to be improvements.

#### The `f` callback is eliminated in this phase

The `f` callback in `infer_map` / `add_type_mapping` lets callers filter or modify type
mappings before they're stored. There are currently two callers of `infer_map`:

1. `infer()` (generics.rs) → uses the identity callback `|(_, _, ty)| Some(ty)`. No-op.
1. `infer_argument_constraints` (bind.rs) → uses the preferred_type_mappings callback.

Caller #1 is trivially compatible — it's already a pass-through. Caller #2 is the only
non-trivial use. The callback does two things:

- Returns `None` to suppress the mapping when the inferred type is assignable to the preferred
    type (the preferred type "wins")
- Sets `assignable_to_declared_type = false` as a side effect when the inferred type is NOT
    assignable to the preferred type

Both of these are about the preferred type mechanism, not about the `f` callback itself. Once
the builder maintains a constraint set, we can replace this per-mapping filtering with a
constraint-set-level check:

1. **Before argument inference**: Seed the preferred types into the builder (via
    `insert_type_mapping`, which creates equality constraints — already done today).
1. **During argument inference**: Call `infer()` instead of `infer_map()` — no callback needed.
    Each argument's inferred type becomes a constraint AND'd into the pending set alongside
    the preferred type constraints.
1. **After argument inference**: Check whether the pending constraint set is satisfiable. If
    the preferred types and argument types are compatible, the set is satisfiable. If not
    (e.g., preferred `T ≤ int` but argument gives `T ≥ str`), the set is unsatisfiable. This
    replaces the `assignable_to_declared_type` flag.
1. **Retry logic**: If unsatisfiable, create a fresh builder without preferred types and
    re-infer from arguments alone (same as today's retry).

This eliminates `infer_map` and the `f` callback entirely from `SpecializationBuilder`. The
`add_type_mapping` internal method also loses its `f` parameter.

The callable/protocol arms in `infer_map_impl` currently pass `f` through to
`add_type_mappings_from_constraint_set`. Once we switch to direct constraint-set conjunction
(Step 5.3), those arms AND their local constraint sets directly into `self.pending` — no `f`
callback is involved. This is the natural behavior: the solver combines preferred type
constraints with callable/protocol constraints, and satisfiability is checked at the end.

#### ParamSpec "first wins" semantics

The current `add_type_mapping` and `insert_type_mapping` both have special handling for
ParamSpec: if a mapping already exists for the typevar, the new one is silently dropped. This
"first wins" semantics can't be expressed as constraint conjunction (ANDing two ParamSpec
constraints would produce their intersection).

**Mitigation**: Keep a `HashSet<BoundTypeVarIdentity>` of ParamSpec typevars that have already
been constrained. Before creating a constraint for a ParamSpec typevar, check the set; if
already present, skip. This preserves existing behavior.

**Note**: The TODO in the existing code acknowledges this is a limitation — ParamSpecs should
ideally be solved to a common behavioral supertype. The "first wins" workaround is preserved
for now, not made worse.

#### Steps

**Step 5.1 ✅**: Eliminated the `f` callback from `infer_map` / `add_type_mapping`.

Implementation details:

1. `infer_argument_constraints` (`bind.rs`) now calls `builder.infer()` instead of
    `builder.infer_map()`.
1. After argument inference, preferred-type compatibility is checked post-hoc against the
    builder's current inferred result for each preferred typevar.
1. To preserve the old HashMap-backed semantics **without mutating the builder's internal
    state**, the final `build_with(...)` hook in `bind.rs` now prefers the declared type when the
    builder's inferred result is assignable to it. If the inferred result is incompatible and the
    typevar is **not** in `partially_specialized_declared_type`,
    `assignable_to_declared_type` is set to `false`. Partially-specialized preferred types keep
    the unioned mapping, matching the previous callback-driven behavior.
1. The retry logic is unchanged: if the preferred types are not compatible, inference is retried
    with a fresh builder that ignores them.
1. Removed the callback plumbing from `SpecializationBuilder`:
    - deleted `infer_map`
    - removed the `f` parameter from `add_type_mapping`
    - removed the `f` parameter from `infer_map_impl`
    - removed the `f` parameter from `add_type_mappings_from_constraint_set`
    - removed the `f` parameter from callable/protocol inference helpers
    - removed the `TypeVarAssignment` type alias
1. Added a temporary read-only query on `SpecializationBuilder` to support the post-hoc
    compatibility check. This helper is expected to go away once Phase 5 switches the builder to
    a `ConstraintSet`, at which point the compatibility test becomes a satisfiability check on
    `self.pending`.

**Validation**:

- `cargo nextest run -p ty_python_semantic --cargo-profile fast-test`
- `cargo nextest run -p ty_python_semantic -p ty_ide --cargo-profile fast-test`
- `/home/dcreager/bin/jpk run -a`

**Step 5.2 ✅**: Added the pending constraint-set scaffolding to `SpecializationBuilder`.

Implementation details:

1. Added `pending: ConstraintSet<'db, 'c>` to `SpecializationBuilder`, initialized to the
    always-true constraint set via `ConstraintSet::from_bool(constraints, true)`.
1. Added `paramspec_seen: FxHashSet<BoundTypeVarIdentity<'db>>` to hold the forthcoming
    ParamSpec "first wins" tracking state.
1. Kept the existing `types` HashMap intact as the active specialization state for now; the new
    fields are scaffolding for the dual-write transition in Steps 5.3–5.5.

**Validation**:

- `cargo nextest run -p ty_python_semantic --cargo-profile fast-test`
- `cargo nextest run -p ty_python_semantic -p ty_ide --cargo-profile fast-test`
- `/home/dcreager/bin/jpk run -a`

**Step 5.3 ✅**: Converted `add_type_mapping` to dual-write into the pending constraint set.

Implementation details:

1. `add_type_mapping` still updates the existing HashMap-backed state via `insert_type_mapping`,
    preserving the current specialization behavior.
1. It now also maps variance to constraint polarity and ANDs the corresponding constraint into
    `self.pending`:
    - Covariant: `(ty, object)`
    - Contravariant: `(Never, ty)`
    - Invariant: `(ty, ty)`
    - Bivariant: no pending constraint
1. The new constraint is created with `ConstraintSet::constrain_typevar(...)` and intersected into
    `self.pending` via `self.pending.intersect(...)`.
1. ParamSpec tracking now uses `paramspec_seen` to preserve today's "first wins" behavior for the
    pending constraint path as well: after the first ParamSpec occurrence, subsequent
    `add_type_mapping` calls for the same bound ParamSpec skip pending-constraint updates.

This leaves `insert_type_mapping` and `add_type_mappings_from_constraint_set` for later steps, so
only the `add_type_mapping` path is dual-written so far.

**Validation**:

- `cargo nextest run -p ty_python_semantic --cargo-profile fast-test`
- `cargo nextest run -p ty_python_semantic -p ty_ide --cargo-profile fast-test`
- `/home/dcreager/bin/jpk run -a`

**Step 5.4 ✅**: Added direct pending-set conjunction for constraint-set-based inference, while
keeping the existing HashMap extraction path.

Implementation details:

1. `add_type_mappings_from_constraint_set` still performs the existing
    extract-solutions-then-reinsert logic for the HashMap-backed specialization state, and still
    returns only `Result<(), ()>`.
1. Callers now dual-write into `self.pending` by intersecting the same local constraint sets that
    were passed into that helper:
    - non-overloaded / single-ParamSpec cases intersect the single local set directly
    - overloaded callable cases OR together all satisfiable overload-local sets, then intersect the
        combined set into `self.pending`
1. The protocol-inference path in `infer_map_impl` likewise intersects `self.pending` only when
    the local constraint set is satisfiable, preserving the existing “unsatisfiable means no
    inference” behavior.
1. `remove_noninferable(...)` remains part of the old HashMap extraction path for now. Pending-set
    normalization is deferred to the later pending-solve path rather than being threaded through
    this helper's API.

This preserves current test behavior by **adding** the new pending-set updates without removing the
existing HashMap-based extraction. The old extraction path remains in place until Step 5.8.

**Validation**:

- `cargo nextest run -p ty_python_semantic --cargo-profile fast-test`
- `cargo nextest run -p ty_python_semantic -p ty_ide --cargo-profile fast-test`
- `/home/dcreager/bin/jpk run -a`

**Step 5.5**: Convert `insert_type_mapping` to create constraints.

`insert_type_mapping` (used by `bind.rs` to seed preferred types and by
`infer_collection_literal` for TCX injection) currently inserts directly into the HashMap.
Convert it to create an equality constraint `constrain_typevar(T, ty, ty)` and AND it into
the pending set. (Equality because preferred types and TCX injections represent specific type
assignments, not directional bounds.)

During the transition, update both HashMap and constraint set.

**Step 5.6**: Add an internal “solve pending set” path for specialization construction.

There is no separate `build()` method anymore; all current callers go through `build_with`.
Introduce an internal helper that solves `self.pending` via `ConstraintSet::solutions_with(...)`,
using `PathBounds::default_solve(...)` as the fallback policy, and materializes per-typevar
specialization entries (`Some(solution)` or `None`) for `specialize_recursive`.

Keep the existing HashMap-backed path alongside temporarily for comparison during testing.

**Step 5.7**: Update `build_with` to use the pending constraint set.

Change `build_with` to drive off the helper from Step 5.6. Internally this means iterating the
pending set's per-path solutions, calling the existing builder-level hook with real lower/upper
bounds, and unioning path-wise results just as the hybrid code does today.

Validate both current callers:

- `maybe_promote` in `bind.rs`
- the collection-literal singleton-promotion hook in `infer/builder.rs`

The `maybe_promote` hook should work correctly with real bounds:

- For typevars with a single covariant constraint: lower = inferred type, upper = object.
    Hook sees `(typevar, Literal[1], object)` and can promote to `int`.
- For typevars with both lower and upper bounds: hook can check whether the promoted type
    is within the upper bound.

The collection-literal hook should continue to look only at the lower bound when there is no TCX.

**Step 5.8**: Remove the HashMap field and old code paths.

Once tests pass with the constraint-set-based `build_with`:

- Remove the `types: FxHashMap` field
- Remove the HashMap update code from `add_type_mapping` and `insert_type_mapping`
- Remove the old `add_type_mappings_from_constraint_set` method entirely
- Remove the `paramspec_seen` set if ParamSpec handling has been integrated into the constraint
    logic (or keep it as a necessary workaround)

At this point, `SpecializationBuilder` fields should be:

```rust
pub(crate) struct SpecializationBuilder<'db, 'c> {
    db: &'db dyn Db,
    constraints: &'c ConstraintSetBuilder<'db>,
    inferable: InferableTypeVars<'db, 'db>,
    pending: ConstraintSet<'db, 'c>,
    // ParamSpec workaround (if still needed)
    paramspec_seen: FxHashSet<BoundTypeVarIdentity<'db>>,
}
```

Also update the `assignable_to_declared_type` check from Step 5.1 to use constraint set
satisfiability: `builder.is_satisfiable()` (checking `!self.pending.is_never_satisfied(db)`)
replaces the post-hoc per-typevar assignability check.

### Phase 6: Migrate `infer_map_impl` arms to constraint sets (optional cleanup)

Status: Not started
**Difficulty: Hard** — many arms with subtle heuristics, each potentially causing behavioral
changes that require test updates. Can be done incrementally (one arm at a time).
**Dependencies: Phase 5** (the builder must maintain a constraint set internally, and the `f`
callback must be eliminated — both done in Phase 5).

After Phase 5, the `infer_map_impl` arms still walk the type structure manually and call
`add_type_mapping`, which now creates constraints. This works correctly — each arm's output
flows through the constraint set solver. However, the arms duplicate logic that the CSA relation
already handles (MRO walking, union/intersection decomposition, bound checking, etc.).

This phase replaces the manual type-walk arms with calls to
`actual.when_constraint_set_assignable_to(formal, ...)`, then AND's the resulting constraint set
into `self.pending`. This eliminates code duplication and makes `infer_map_impl` trivially small.

The CSA relation already handles most cases that `infer_map_impl` has specialized arms for:

- **Union handling**: CSA's `when_any`/`when_all` on union elements naturally handles the cases
    that `infer_map_impl` has complex heuristics for (single-typevar-in-union, single-assignable-
    element, etc.). Worked example: `str | None ≤ T | None` via CSA correctly infers `T ≥ str`
    without any special-case logic — `str ≤ T` produces the constraint, `None ≤ None` is
    unconditionally true, so T is only constrained from below by `str`.
- **TypeVar with bounds/constraints**: The CSA handler at line 450 directly creates constraints.
- **NominalInstance MRO walk**: `has_relation_to_impl` already walks MRO for specialization
    comparisons.
- **TypeAlias expansion**: Handled by the relation code.

**Step 6.1**: Replace the TypeVar match arm with CSA. Currently manually checks
bounds/constraints and calls `add_type_mapping`.

**Step 6.2**: Replace the NominalInstance/specialization walk arm with CSA.

**Step 6.3**: Replace the Union/Intersection arms with CSA.

**Step 6.4**: Replace remaining arms (TypeAlias, tuple, SubclassOf, LiteralValue fallback, etc.)
one by one.

**Step 6.5**: Once all arms are migrated, `infer_map_impl` reduces to:

```rust
fn infer_map_impl(...) -> Result<(), SpecializationError<'db>> {
    let when = actual.when_constraint_set_assignable_to(self.db, formal, ...);
    self.pending.intersect(self.db, self.constraints, when);
    Ok(())
}
```

Steps 6.1–6.4 are independent of each other and can be done in any order.

**Note**: Each step should be validated by running the full test suite. Behavioral differences
from CSA vs old solver are expected (usually more precise), and may require test updates.

### Phase 7: Replace `preferred_type_mappings` with constraint set conjunction

Status: Not started
**Difficulty: Medium** — conceptually straightforward once Phase 5 is complete, but may require
adjusting the fallback logic.
**Dependencies: Phase 5** (the builder must maintain a constraint set).

After Phase 5, the preferred type mechanism works as follows:

1. Extract preferred types from the TCX by solving `return_ty ≤ tcx` and filtering solutions
    (variance, inferable typevars, concrete content checks)
1. Seed them into the builder via `insert_type_mapping` (creating equality constraints)
1. Infer argument types via `infer()` (creating additional constraints)
1. Check satisfiability of the combined constraint set; retry without preferred types if
    unsatisfiable

This works correctly but is more complex than necessary. The TCX produces a constraint set
(`return_ty.when_constraint_set_assignable_to(tcx, ...)`), but instead of using that constraint
set directly, we extract solutions, filter them (variance, inferable typevars, concrete content),
and re-inject them as individual equality constraints.

This phase replaces that with direct conjunction:

1. Let `tcx_set = return_ty.when_constraint_set_assignable_to(tcx, ...)`
1. AND `tcx_set` into the builder's pending constraint set before argument inference
1. Infer argument types via `infer()` (AND'd into the same pending set)
1. Check satisfiability; if unsatisfiable, create a fresh builder without `tcx_set` and retry

This eliminates the ad-hoc solution-level filtering (variance, inferable typevars, concrete
content), since the constraint solver naturally resolves the tension between TCX preferences and
argument constraints. It also removes `insert_type_mapping`, the `preferred_type_mappings`
HashMap, the `partially_specialized_declared_type` set, and the solution extraction/filtering
logic in `infer_specialization`.
