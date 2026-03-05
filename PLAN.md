# Plan: Migrate SpecializationBuilder from type_mappings HashMap to ConstraintSet

## Status: In progress (Phases 1–4 complete; all tests passing)

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
    `SpecializationBuilder` purely to query per-typevar information — it never calls `build()` to
    produce a final specialization (or only builds an intermediate one for downstream use). In the
    new world, these cases **bypass `SpecializationBuilder` entirely**, instead creating a
    `ConstraintSet` directly via `when_constraint_set_assignable_to` and querying it for
    per-typevar solutions.

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

### 1. `preferred_type_mappings` + `infer_argument_types` callback (`call/bind.rs:3706-3897`)

Pattern: 1 (constraint conjunction)

**Builds a specialization**: Yes — this is the main specialization for a generic function call.

**Current behavior**: Reverse-infers from the type context (the declared return type annotation)
to get "preferred" types per typevar. Stores them in a HashMap. Then in `infer_argument_types`, a
callback checks each argument-inferred type against the preferred type — if assignable, it's
dropped (the preferred type wins). If not assignable, `assignable_to_declared_type` is set to
false, triggering a retry without preferences.

**New approach**: The type context inference produces a constraint set (via forward CSA:
`return_type.when_constraint_set_assignable_to(tcx, ...)`), which is conjoined with the argument
inference constraints. "Is the argument's inference compatible with the preference?" becomes "is
the combined constraint set satisfiable?". The retry logic becomes: try with TCX constraints
conjoined; if unsatisfiable, drop them and retry.

**Worked examples**:

- `f[T](x: T) -> list[T]` with `result: list[int] = f(True)`: TCX gives `T ≤ int`, arg gives
    `T ≥ bool`, combined `bool ≤ T ≤ int`, satisfiable. Solution hook picks upper bound for
    non-covariant → `T = int`. Same as current.
- `f[T](x: T) -> list[T]` with `result: list[int] = f("hello")`: TCX gives `T ≤ int`, arg
    gives `T ≥ str`, combined unsatisfiable (`str` not `≤ int`). Retry without TCX → `T = str`.
    Same as current.

**Subtlety: covariant filter.** Currently the callback filters out covariant typevars from TCX
inference. Without the filter, we'd get additional upper-bound constraints that are benign (the
solution hook picks lower bounds for covariant typevars anyway), but could cause spurious
unsatisfiability, triggering unnecessary retries. Not a correctness issue — the retry gives the
correct result — but a performance concern worth monitoring.

**Subtlety: `partially_specialized_declared_type`.** The current code tracks whether TCX-derived
types contain unspecialized typevars, and softens the "not assignable" error for those. In the
constraint set world, we'd need to check TCX-derived constraints for unspecialized typevars
before conjoining them. Could pre-filter the types going into the TCX assignability check, or
examine the constraint set's bounds for unspecialized typevars post-hoc. Not a blocker but
requires design thought about where this check lives.

**Eliminates**: `type_mappings()`, the `f` callback in `infer_map`, the
`preferred_type_mappings` map, and the `infer_reverse_map` call (replaced by forward CSA).

### 2. `maybe_promote` via `mapped()` (`call/bind.rs:3841`)

Pattern: 2 (solution extraction hook)

**Builds a specialization**: Yes — same builder as #1, this is the `build` step.

**Current behavior**: After inference, `mapped()` clones the type mapping, applies
`maybe_promote` to each entry. `maybe_promote` checks the typevar's variance in the return type
and its declared bounds, and potentially promotes literals (e.g., `Literal[1]` → `int`).

**New approach**: The solution extraction hook receives the lower/upper bounds for each typevar.
The caller closes over the return type, call expression TCX, etc. The promotion logic becomes: if
the lower bound is a literal type, and the typevar appears in a non-covariant position in the
return type, and the promoted type is still within the upper bound, choose the promoted type.

**Feasibility**: Straightforward. The hook is a closure that can capture whatever context it
needs.

**Eliminates**: `mapped()`.

### 3. `with_default()` in bidirectional argument inference (`infer/builder.rs:9425-9453`)

Pattern: 3 (standalone constraint set query)

**Builds a specialization**: Technically yes, but only as an intermediate — it creates a partial
specialization to apply to parameter types for downstream bidirectional inference. It is not the
"main" specialization of the call.

**Current behavior**: Creates a temporary builder, calls `infer_reverse(declared_return_ty, return_ty)` to get TCX-derived mappings, fills in `UnspecializedTypeVar` for unmapped typevars
via `with_default`, and builds a specialization to partially specialize parameter types.

**New approach**: This becomes a direct constraint set query:

1. `return_ty.when_constraint_set_assignable_to(declared_return_ty, ...)` to get a constraint set
1. Extract solutions via `solutions()`
1. Create a specialization via `GenericContext::specialize_partial`, passing `None` for unsolved
    typevars (which `fill_in_defaults` will handle) or using the `UnspecializedTypeVar` marker
    directly.

No `SpecializationBuilder` needed.

**Feasibility**: Straightforward.

**Eliminates**: This use of `with_default()` and `infer_reverse`.

### 4. `infer_reverse_map_impl`'s internal builder (`generics.rs:2480-2510`)

Pattern: Goes away entirely (internal implementation detail of `infer_reverse`)

**Builds a specialization**: No — purely internal to the reverse inference mechanism.

**Current behavior**: Creates a temporary builder with synthetic typevars, calls `infer` in the
forward direction, extracts the map, then uses those mappings to map synthetic typevars back to
actual typevars, recursing into reverse inference.

**Why it goes away**: This entire mechanism exists because the old solver can only do forward
inference. The CSA relation handles the recursive specialization walk naturally. For
`Container[Box[T]] ≤ Container[Box[int]]`, the relation walks Container's specialization, then
Box's, producing `T ≤ int` (or `T = int` for invariant containers). No synthetic typevars
needed.

**Eliminates**: `infer_reverse`, `infer_reverse_map`, `infer_reverse_map_impl`,
`into_type_mappings()` as used here, the `UniqueSpecialization` TypeMapping variant.

### 5. `visit_specialization_impl` (`types.rs:1912-1921`)

Pattern: 3 (standalone constraint set query)

**Builds a specialization**: No — extracts per-typevar type context narrowing for downstream use.

**Current behavior**: Creates a temporary builder, calls `infer_reverse(tcx, alias_instance)`,
extracts mappings as `tcx_mappings`, uses them to provide per-typevar type context narrowing.

**New approach**: Direct constraint set query, no `SpecializationBuilder` needed:

1. `alias_instance.when_constraint_set_assignable_to(tcx, ...)` to get a constraint set
1. Extract solutions via `solutions()`
1. Build the `tcx_mappings` lookup from solutions

**Feasibility**: Straightforward.

### 6. `infer_collection_literal_type` (`infer/builder.rs:10245-10414`)

This function uses **two** builders. They fall into different patterns.

#### 6a. First builder: TCX query (`infer/builder.rs:10245-10289`)

Pattern: 3 (standalone constraint set query)

**Builds a specialization**: No — extracts per-typevar TCX constraints (`elt_tcx_constraints`)
and per-typevar variance (`elt_tcx_variance`) for downstream use.

**Current behavior**: Creates a temporary builder, calls `infer_reverse_map` from the TCX. The
callback does two things: (a) extracts per-typevar type context constraints, and (b) tracks
per-typevar variance. These outputs feed into the second builder.

**New approach**: Direct constraint set query, no `SpecializationBuilder` needed:

1. `collection_instance.when_constraint_set_assignable_to(tcx, ...)` to get a constraint set
1. Extract per-typevar types from `solutions()`
1. Compute per-typevar variance from the type structure using existing `variance_of` methods on
    the collection class's typevars

The callback's partially-specialized-typevar filtering is handled post-hoc on the extracted
solutions.

**Variance tracking concern**: This is where the most friction exists. In the callback-based
approach, variance is reported per-typevar as the callback fires. In the constraint set approach,
variance is *implicit in the bound structure* — a covariant typevar produces only an upper bound
(`T ≤ tcx_type`), a contravariant one only a lower bound, invariant produces both.

**Problem with extracting variance from constraint sets**: For multi-path BDDs (disjunctive
solutions), a typevar might have different bounds on different paths. On one path it might have
only an upper bound (covariant), on another it might have equality. "What variance did this
typevar appear at?" is not well-defined for multi-path constraint sets.

**Resolution**: Compute variance directly from the type structure rather than from the constraint
set. We already have `variance_of` methods that return the variance of a typevar within a type.
For `collection_instance ≤ tcx`, we know the generic context's typevars and can compute their
variance in the collection class statically. This is simpler and more correct than deriving
variance from the constraint structure.

#### 6b. Second builder: element inference (`infer/builder.rs:10293-10414`)

Pattern: 1 (constraint conjunction)

**Builds a specialization**: Yes — this is the final specialization for the collection type.

**Current behavior**: Creates a builder, adds TCX constraints via `infer(TypeVar(elt_ty), elt_tcx)`, adds per-element inferred types via `infer(TypeVar(elt_ty), inferred_elt_ty)`, then
calls `build(generic_context)`.

**New approach**: The TCX constraints from builder 6a (now a standalone constraint set query)
are conjoined into this builder's pending state (Pattern 1). Element inferences continue to be
added via the normal `infer` path. The `build` call remains unchanged (no custom hook needed).

## The `solutions()` function and the extraction hook

The current `solutions()` implementation (`constraints.rs:3023`) already computes per-typevar
lower/upper bounds for each BDD path, then makes a hardcoded choice:

- For bounded typevars: prefer the lower bound if non-`Never`; else intersect upper bounds with
    the typevar's declared upper bound.
- For constrained typevars: find the unique compatible constraint.

The proposed Category 2 hook would replace this hardcoded policy. The hook signature would be
something like:

```rust
fn build_with(
    &self,
    generic_context: GenericContext<'db>,
    choose: impl Fn(
        BoundTypeVarInstance<'db>,
        /* lower */ Type<'db>,
        /* upper */ Type<'db>,
    ) -> Option<Type<'db>>,
) -> Specialization<'db>
```

Where `None` means "use the default for this typevar."

**Multi-path BDD handling**: For constraint sets with disjunctive solutions (multiple BDD paths),
we have two options:

1. Run the hook per-path, then combine (union) the per-path results
1. First combine bounds across paths, then run the hook once

Option 1 preserves more information; option 2 is simpler. The current code effectively does
option 1 (iterates paths, computes per-path solutions, then `add_type_mappings_from_constraint_set`
unions them via `add_type_mapping`). The hook-based approach should follow the same pattern:
iterate paths, call the hook for each path's per-typevar bounds, combine results.

## Feasibility summary

| Call site                                           | Pattern   | Feasible? | Risk/Concern                                                  |
| --------------------------------------------------- | --------- | --------- | ------------------------------------------------------------- |
| `preferred_type_mappings` + callback                | 1         | Yes       | `partially_specialized_declared_type` needs clean replacement |
| `maybe_promote` via `mapped()`                      | 2         | Yes       | Straightforward                                               |
| `with_default()` / bidirectional arg inference      | 3         | Yes       | Straightforward                                               |
| `infer_reverse_map_impl` internal                   | Goes away | Yes       | —                                                             |
| `visit_specialization_impl`                         | 3         | Yes       | Straightforward                                               |
| `infer_collection_literal_type` (TCX query)         | 3         | Yes       | Variance: compute from type structure, not constraint set     |
| `infer_collection_literal_type` (element inference) | 1         | Yes       | TCX constraints conjoined into builder                        |

No fundamental blockers. Main design challenges:

1. Solution extraction hook API (Pattern 2) and its interaction with multi-path BDDs
1. Preserving the `partially_specialized_declared_type` heuristic
1. Behavioral differences from CSA vs old solver (usually more precise, but might need test updates)

## Completeness verification

All public methods on `SpecializationBuilder` are accounted for:

| Method               | Callers                                                                | Plan coverage                 |
| -------------------- | ---------------------------------------------------------------------- | ----------------------------- |
| `new`                | 5 external sites + 1 internal                                          | All covered                   |
| `type_mappings`      | `call/bind.rs:3762`                                                    | Call site #1                  |
| `into_type_mappings` | `types.rs:1921`, `infer/builder.rs:10289`, `generics.rs:2483`          | Call sites #4, #5, #6         |
| `mapped`             | `call/bind.rs:3841`                                                    | Call site #2                  |
| `with_default`       | `infer/builder.rs:9450`                                                | Call site #3                  |
| `build`              | `call/bind.rs:3842`, `infer/builder.rs:9453`, `infer/builder.rs:10414` | All covered                   |
| `infer`              | `infer/builder.rs:10321,10349,10399`, `generics.rs:2482`               | Collection literal + internal |
| `infer_map`          | `call/bind.rs:3864`                                                    | Call site #1                  |
| `infer_reverse`      | `types.rs:1918`, `infer/builder.rs:9432`                               | Call sites #3, #5             |
| `infer_reverse_map`  | `call/bind.rs:3734`, `infer/builder.rs:10264`                          | Call sites #1, #6             |

All `infer_reverse` / `infer_reverse_map` callers have been verified to be replaceable by
forward CSA checks. In each case:

- **`visit_specialization_impl`** (`types.rs:1918`): `infer_reverse(tcx, alias_instance)` where
    `alias_instance` has inferable typevars from the identity specialization. Replacement:
    `alias_instance.when_constraint_set_assignable_to(tcx, ...)`. Verified: the old code falls
    through `infer_reverse_map_impl` to `infer_map_impl(alias_instance, tcx)` since `tcx` has no
    typevars to create synthetics from. CSA equivalent produces the same constraints.

- **Bidirectional argument inference** (`infer/builder.rs:9432`):
    `infer_reverse(declared_return_ty, return_ty)` where `return_ty` has inferable typevars.
    Replacement: `return_ty.when_constraint_set_assignable_to(declared_return_ty, ...)`.
    Verified: the old code falls through to `infer_map_impl(return_ty, declared_return_ty)` since
    `declared_return_ty` has no inferable typevars. CSA equivalent is identical.

- **`infer_specialization` preferred types** (`call/bind.rs:3734`):
    `infer_reverse_map(tcx, return_ty, callback)`. Replacement:
    `return_ty.when_constraint_set_assignable_to(tcx, ...)`. The callback's three concerns
    (covariant filter, unspecialized typevar filter, partially-specialized tracking) are handled
    post-hoc: variance from type structure, unspecialized typevar check on solutions.

- **`infer_collection_literal_type`** (`infer/builder.rs:10264`):
    `infer_reverse_map(tcx, collection_instance, callback)`. Replacement:
    `collection_instance.when_constraint_set_assignable_to(tcx, ...)`. Verified: for the simple
    case (`tcx=list[int]`, `collection_instance=list[T]`), the old code falls through to
    `infer_map_impl(list[T], list[int])`. For the complex case (`tcx=list[U]` with non-inferable
    U), CSA produces `T ≤ U`, solutions give `T = U`, post-hoc filter discards because U has
    unspecialized typevars. Variance: computed from type structure via `variance_of`, matches the
    old callback's reported variance.

## Key file locations

- **`SpecializationBuilder`**: `crates/ty_python_semantic/src/types/generics.rs` ~line 1719
- **`ConstraintSet` and `solutions()`**: `crates/ty_python_semantic/src/types/constraints.rs`
    - `solutions()` inner implementation at ~line 3023, with the `Bounds` struct
    - `constrain_typevar()` at ~line 279
    - `Solutions` / `TypeVarSolution` types at ~line 3611
- **CSA typevar handling**: `crates/ty_python_semantic/src/types/relation.rs` ~line 450
    - This is the `ConstraintSetAssignability` early-return that creates constraints for typevars
        on either side of a comparison. This is what makes forward CSA work as a replacement for
        `infer_reverse`.
- **`infer_specialization`** (call sites #1, #2): `crates/ty_python_semantic/src/types/call/bind.rs` ~line 3706
- **`infer_argument_types`**: same file, ~line 3848
- **`visit_specialization_impl`** (call site #5): `crates/ty_python_semantic/src/types.rs` ~line 1853
- **Bidirectional argument inference** (call site #3): `crates/ty_python_semantic/src/types/infer/builder.rs` ~line 9425
- **`infer_collection_literal_type`** (call site #6): same file, ~line 10245
- **`variance_of` methods**: `crates/ty_python_semantic/src/types/variance.rs` and various
    type-specific files (instance.rs, class.rs, signatures.rs, etc.)

## Validation

After each step, run:

```sh
cargo nextest run -p ty_python_semantic --cargo-profile fast-test
```

For steps that might have broader impact (especially Phase 5 arms), also run:

```sh
cargo nextest run --cargo-profile fast-test
```

```sh
cargo nextest run -p ty_python_semantic -p ty_ide --cargo-profile fast-test
```

For deeper validation (especially Phase 5 arms or Phase 6), run ecosystem analyses using the
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
  │                                              Phase 5 (migrate infer_map_impl arms)
  │                                                        │
  └────────────────────────────────────────────────────────►│
                                                           ▼
                                                  Phase 6 (switch internal repr)
```

Key observations:

- **Phase 1** and **Phase 3** have no dependencies and can start immediately, in parallel.
- **Phase 2** depends on Phase 1 (needs the hook API).
- **Phase 4** steps have mixed dependencies: Step 4.1 is independent; Step 4.2 depends on
    Phase 2 (same builder) and Phase 3 (eliminates remaining `infer_reverse` callers);
    Step 4.3 depends on Steps 3.2 and 4.1; Step 4.4 depends on all other Phase 3/4 steps.
- **Phase 5** depends on Phase 4 (callers must be migrated before we change `infer_map_impl`).
- **Phase 6** depends on Phase 5 (all constraints must flow through constraint sets).

### Phase 1: Design the solution extraction hook API

Status: Not started

Status: Complete ✅
**Difficulty: Medium** — requires design decisions about multi-path BDD handling and the hook
signature, but the implementation is modest (refactoring existing code in `solutions()`).
**Dependencies: None** — can start immediately.

The `solutions()` function in `constraints.rs` already computes per-typevar lower/upper bounds
for each BDD path, then makes a hardcoded choice about which type to return. The hook replaces
that hardcoded choice.

**Step 1.1 ✅**: Refactored `solutions()` to separate bounds computation from solution selection.
Extracted to module-level helpers:

- `Bounds` struct: accumulates raw lower/upper bounds per typevar
- `TypeVarBounds` struct: materialized lower/upper bounds (union of lowers, intersection of
    uppers)
- `compute_path_bounds()`: computes sorted BDD paths and materializes per-typevar bounds
- `default_solve()`: the default solution selection logic for a single typevar
- `solve_paths()`: applies a per-typevar solver function across all paths

`solutions_inner` now calls `compute_path_bounds` + `solve_paths(... default_solve)`.

**Step 1.2 ✅**: Designed and implemented the hook signature. Added `solutions_with` on
`ConstraintSet` (and the internal `NodeId`/`InteriorNode` dispatch):

- Hook: `FnMut(BoundTypeVarInstance, Type, Type) -> Option<Type>`
    - Receives typevar + materialized lower/upper bounds per BDD path
    - Returns `Some(ty)` to override, `None` to fall back to `default_solve`
- For multi-path BDDs, the hook is called per-path; `solve_paths` collects valid paths
- `Solutions<S>` is now generic over the container type: cached solutions use
    `Solutions<Ref<'c, Vec<Solution<'db>>>>`, hook-based solutions use
    `Solutions<Vec<Solution<'db>>>`

**Step 1.3 ✅**: Implemented `build_with` on `SpecializationBuilder` alongside existing `build`.
Initially backed by the HashMap:

- Mapped typevars: hook receives `(typevar, mapped_ty, mapped_ty)` (equality bounds)
- Unmapped typevars: hook receives `(typevar, Never, object)` (open bounds)
- `Some(ty)` from hook overrides the mapped type; `None` uses default
- Replaces the `mapped(...).build(...)` pattern in a single step

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

These sites currently use temporary `SpecializationBuilder` instances just to query per-typevar
information. They don't build a final specialization (or only build an intermediate one). In the
new world, they bypass `SpecializationBuilder` entirely, using `ConstraintSet` APIs directly.

These are good candidates for early migration because they are self-contained — changing them
doesn't affect the `SpecializationBuilder` API or its other callers.

**Step 3.1 ✅**: Migrated `visit_specialization_impl` (`types.rs`):

- Replaced `infer_reverse(tcx, alias_instance)` with forward CSA:
    `alias_instance.when_constraint_set_assignable_to(tcx, ...)`.
- Extracted per-typevar types from the resulting constraint set's `solutions()`.
- Built the `tcx_mappings` lookup from solutions, unioning across BDD paths.
- Removed the temporary `SpecializationBuilder`. Also added `Solutions` to the imports from
    `constraints` in `types.rs`.

**Step 3.3 ✅**: Migrated bidirectional argument inference (`infer/builder.rs`):

- Replaced `infer_reverse(declared_return_ty, return_ty)` with forward CSA:
    `return_ty.when_constraint_set_assignable_to(declared_return_ty, ...)`.
- Extracted solutions via `solutions()`, built `tcx_mappings` HashMap.
- Created the intermediate specialization via `GenericContext::specialize_recursive`,
    using `Some(mapped_ty)` for solved typevars and
    `Some(Type::Dynamic(DynamicType::UnspecializedTypeVar))` for unsolved ones.
- Removed the temporary `SpecializationBuilder` and `with_default` call.
- Also added `Solutions` to the imports from `constraints` in `builder.rs`.

**Step 3.2 ✅**: Migrated the TCX query in `infer_collection_literal`
(`infer/builder.rs`):

- Replaced `infer_reverse_map(tcx, collection_instance, ...)` with forward CSA:
    `collection_instance.when_constraint_set_assignable_to(tcx, ...)`.
- Extracted per-typevar types from `solutions_with()` (the hook-based variant).
- **Variance approach**: Determined variance from the constraint *bounds* rather than from the
    collection class type structure. The `solutions_with` hook receives raw lower/upper bounds
    per typevar per BDD path:
    - `lower = Never` (no lower bound) → covariant position
    - `upper = object` (no upper bound) → contravariant position
    - Both bounds set → invariant position
        This correctly handles cases where the TCX type is a covariant superclass of the collection
        (e.g., `Sequence[Any]` as TCX for `list[T]`), where the old reverse inference couldn't find
        the relationship at all but the CSA correctly walks the MRO.
- Applied partially-specialized-typevar filtering post-hoc on solutions.
- **Key invariant**: variance entries are retained only for typevars that have actual constraint
    entries (via `elt_tcx_variance.retain()`). This prevents false covariant variance from being
    recorded for typevars whose solutions were filtered out by the unspecialized-typevar check.
- Removed the first temporary `SpecializationBuilder`.
- Removed `#[expect(dead_code)]` from `solutions_with` on `ConstraintSet` (now used).
- **Cross-typevar filtering**: The SequentMap's transitivity reasoning can inject inferable
    typevars into solutions. For example, for `dict[_KT, _VT] ≤ dict[str, int | str]`, the
    constraints `_KT ≤ str` and `str ≤ _VT` share `str` as a pivot, deriving `_KT ≤ _VT`.
    This adds `_KT` to `_VT`'s lower bound, producing `_KT | int | str` instead of `int | str`
    and changing union ordering. Fixed by filtering inferable typevars from solutions via
    `filter_union` + `as_typevar` + `is_inferable` — the solution should contain only concrete
    types, not cross-typevar relationships.
- **Test results**: All 1754 tests pass. One test expectation was updated:
    - `literal_promotion.md:220`: `list[Y[Literal[1]]]` → `list[list[Literal[1]]]` — type
        alias `Y` (defined as `type Y[T] = list[T]`) is resolved because the CSA normalizes
        the annotation before creating constraints. Semantically equivalent; confirmed acceptable.

### Phase 4: Migrate Pattern 1 call sites (constraint conjunction) and finish eliminating `infer_reverse`

Status: Complete ✅
**Difficulty: Hard** — Step 4.2 is the most complex migration in the entire plan, touching the
core specialization inference logic with subtle heuristics (`partially_specialized_declared_type`,
covariant filtering, retry logic).
**Dependencies:**

- Step 4.1 has no dependencies (can start any time).
- Step 4.2 depends on Phase 2 (same builder uses `build_with`) and Phase 3 (all other
    `infer_reverse` callers must be migrated first, so we can validate removal).
- Step 4.3 depends on Steps 3.2 (TCX query migrated) and 4.1 (conjoin method exists).
- Step 4.4 depends on Steps 3.1–3.3, 4.2, 4.3 (all `infer_reverse` callers migrated).

**Step 4.1** ✅: Added `conjoin_constraint_set` and `insert_type_mapping` methods to
`SpecializationBuilder`. Note: `conjoin_constraint_set` was subsequently removed (unused after
Step 4.2 migrated to `solutions_with_inferable`). `insert_type_mapping` remains as `pub(crate)`
for use by `bind.rs` to seed the builder with preferred types.

**Step 4.2** ✅: Migrate the `preferred_type_mappings` pattern in `infer_specialization`
(`call/bind.rs:~3730`):

Replaced `infer_reverse_map(tcx, return_ty, ...)` with a forward CSA check:
`return_ty.when_constraint_set_assignable_to(tcx, ...)`. Solutions are extracted via
`solutions_with_inferable`, which handles non-inferable typevars from outer scopes.

Implementation details:

- **CSA handler** (`relation.rs`): No changes — the CSA always constrains all typevars,
    regardless of inferability. Filtering happens at the solution extraction level, not at
    constraint creation. This aligns with the design goal of eventually removing the `inferable`
    parameter from `has_relation_to_impl`, with callers using `satisfied_by_all_typevars` for
    inferable/non-inferable distinction.
- **`is_cyclic_for`** (`constraints.rs`): Like `is_cyclic` but only includes inferable typevars
    in the reachability graph. Non-inferable typevars that appear due to BDD constraint reordering
    are excluded from cycle detection.
- **`solutions_with_inferable`** (`constraints.rs`): Uses `is_cyclic_for` for cycle detection,
    and skips non-inferable typevars during solution extraction (`solve_paths`). This avoids
    `Err(())` from `default_solve` when non-inferable typevar bounds don't satisfy the typevar's
    declared constraints. Cross-typevar propagation in `compute_path_bounds` means inferable
    typevars can get bounds that reference non-inferable typevars.
- **`contains_identity`** (`generics.rs`): Helper method on `InferableTypeVars` for checking
    whether a `BoundTypeVarIdentity` is in the inferable set.
- **Preferred type filtering** (`bind.rs`): Three filters on solutions:
    1. Remove top-level inferable typevars (SequentMap transitivity artifacts)
    1. Remove types with unspecialized typevars (partially specialized contexts)
    1. Skip solutions where no union element is purely concrete (no typevars at any depth).
        This handles cases where the TCX contains non-inferable typevars (e.g.,
        `T@h | list[T@h]` from an outer generic scope) — the CSA produces solutions
        referencing those typevars, but they don't provide useful concrete information.
        Valid cases like `T@_ | int` (concrete `int` alongside outer-scope typevar) are
        preserved because `int` passes the concrete check.

Behavioral change (improvement): `annotations.md:611` — the old code couldn't infer preferred
types through union TCXs (e.g., `list[Any] | None`). The CSA approach correctly infers `T=Any`
from the annotation, changing the revealed type from `list[int] | None` to `list[Any] | None`.
Test expectation updated.

**Step 4.3** ✅: Simplified the TCX injection in `infer_collection_literal_type`'s second builder
(`infer/builder.rs`):

- Replaced `builder.infer(Type::TypeVar(elt_ty), elt_tcx)` with
    `builder.insert_type_mapping(elt_ty, elt_tcx)`, which directly inserts the type mapping
    without going through `infer_map_impl`. This is semantically equivalent for collection
    typevars (which have `object` bounds and always pass the bound check), and removes one
    caller of the `infer`/`infer_map_impl` code path.
- The covariant filtering and Unknown fallback logic remain unchanged.
- Note: the original plan envisioned conjoining the raw CSA constraint set via
    `conjoin_constraint_set`. That approach was not viable here because (a) covariant typevars
    must be excluded from TCX injection (requiring filtering that ConstraintSet doesn't support),
    and (b) typevars without TCX constraints need an explicit Unknown fallback. The direct
    `insert_type_mapping` approach is the correct simplification at this stage; full constraint
    set conjunction will happen in Phase 6 when the builder's internal representation changes.

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

### Phase 5: Migrate `infer_map_impl` arms to constraint sets

Status: Not started
**Difficulty: Hard** — many arms with subtle heuristics, each potentially causing behavioral
changes that require test updates. Can be done incrementally (one arm at a time).
**Dependencies: Phase 4** (callers must be migrated so that the `f` callback and
`preferred_type_mappings` patterns are gone before we change how `infer_map_impl` works).

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

**Step 5.1**: Replace the TypeVar match arm with CSA. Currently manually checks
bounds/constraints and calls `add_type_mapping`.

**Step 5.2**: Replace the NominalInstance/specialization walk arm with CSA.

**Step 5.3**: Replace the Union/Intersection arms with CSA.

**Step 5.4**: Replace remaining arms (TypeAlias, tuple, SubclassOf, LiteralValue fallback, etc.)
one by one.

**Note**: Each step should be validated by running the full test suite. Behavioral differences
from CSA vs old solver are expected (usually more precise), and may require test updates.
Steps 5.1–5.4 are independent of each other and can be done in any order.

### Phase 6: Switch internal representation

Status: Not started
**Difficulty: Medium** — mostly mechanical once all constraints flow through constraint sets,
but may surface edge cases in the interaction between the HashMap and constraint set paths.
**Dependencies: Phase 5** (all `infer_map_impl` arms must use constraint sets).

At this point, the internal HashMap should be nearly vestigial — most or all constraints flow
through the constraint set.

**Step 6.1**: Add a `ConstraintSet` field alongside the `types` HashMap during a brief
transition.

**Step 6.2**: Change `add_type_mapping` to create proper constraints instead of HashMap entries:

- Covariant position: add lower bound `ty ≤ T`
- Contravariant position: add upper bound `T ≤ ty`
- Invariant position: add equality `T = ty`

**Step 6.3**: Update `build` / `build_with` to extract solutions from the constraint set.

**Step 6.4**: Remove the HashMap field, `add_type_mapping`, and
`add_type_mappings_from_constraint_set` (now just an AND on the pending constraint set).

**Step 6.5**: Remove `infer_map` — `infer` becomes a thin wrapper around
`actual.when_constraint_set_assignable_to(formal, ...)` conjoined into the pending set.

### Phase 7: Replace `preferred_type_mappings` with constraint set conjunction

Status: Not started
**Difficulty: Medium** — conceptually straightforward once Phase 6 is complete, but may require
adjusting the fallback logic.
**Dependencies: Phase 6** (the builder must maintain a constraint set).

The `preferred_type_mappings` mechanism in `bind.rs` currently uses a two-phase approach:

1. Extract preferred types from the TCX by solving `return_ty ≤ tcx` and filtering solutions
    (variance, inferable typevars, concrete content checks)
1. During argument inference, prefer TCX types unless argument types are incompatible
1. If incompatible, re-infer from arguments alone

This should be replaced by directly conjoining the TCX constraint set with argument constraints:

1. Let `tcx_set = return_ty.when_constraint_set_assignable_to(tcx, ...)`
1. Let `arg_set = ∧ᵢ (actual_i ≤ formal_i)` for all arguments
1. Try solving `tcx_set ∧ arg_set` (combined)
1. If unsatisfiable, fall back to just `arg_set`

This eliminates the ad-hoc solution-level filtering (variance, inferable typevars, concrete
content), since the constraint solver would naturally resolve the tension between TCX preferences
and argument constraints. It also removes `insert_type_mapping`, `preferred_type_mappings`,
`partially_specialized_declared_type`, and the `assignable_to_declared_type` retry logic.
