# Plan: Factored upper bounds for ty constraint sets

## Status legend and handoff instructions

- `[ ]` Not started
- `[~]` In progress
- `[x]` Complete
- This file is the ground truth for implementation order and dependencies.
- When resuming this plan, first read the relevant code and validate that these status markers are still accurate before making changes.
- Keep this file updated as phases are completed or the design changes.
- Follow repository instructions: create a new `jj` revision before editing, describe revisions with a `[π]` prefix, and use `jj diff --git` / `jj diff --stat` for review.
- When testing the `jax` ecosystem project, **always apply an explicit memory limit** with `ulimit` or an equivalent mechanism so the agent process itself is not OOM-killed.

## Background

We are using `dcreager/deferred-quantification` as the base bookmark, not `main`.
That bookmark introduced deferred existential quantification of constraint sets:
quantification is delayed until solution extraction so that larger constraint sets can still see typevars introduced by generic callables, higher-order function signatures, etc.

This enabled more precise interactions between constraints, but introduced a performance regression:
ty gets OOM-killed when analyzing the `jax` ecosystem project. `main` can analyze `jax` in less than one second.

The pathological cases usually involve overloaded methods and protocols. They produce constraints whose upper bounds are large unions. When two constraints on the same typevar are combined, their upper bounds must be intersected. ty's ordinary `Type` representation is in DNF. Intersecting many large unions therefore requires distributing intersection over union, which can require quadratic or worse time and space.

There is already a stop-gap heuristic during sequent map creation: `ConstraintId::intersect` estimates the distributed upper-bound size and returns `IntersectionResult::CannotSimplify` if it would be too large. The feature-branch baseline now also has an equivalent stop-gap in solution extraction: `ConstraintBoundsBuilder::finish` estimates the size of the accumulated upper-bound intersection and falls back to `Unknown` instead of eagerly materializing a large `IntersectionType`.

Those heuristics are useful temporary protection, but they are not the intended final design. The goal is to introduce a representation that keeps upper bounds factored for longer and only materializes a compact solution witness when required.

## Current relevant code

Primary file:

- `crates/ty_python_semantic/src/types/constraints.rs`

Important current structures and methods:

- `ConstraintBounds<'db>` currently stores:
    - `lower: Option<Type<'db>>`
    - `upper: Option<Type<'db>>`
- `ConstraintBoundsBuilder<'db>` already accumulates upper bounds in a CNF-like form:
    - `upper: FxIndexSet<Type<'db>>`
    - each entry is one upper clause
    - `add_upper` prunes redundant clauses
    - `finish` currently either collapses this to a single `Type` with `IntersectionType::from_elements` or, if the estimated distributed size is too large, falls back to `Unknown`
- `ConstraintId::intersect` currently has a local upper-bound explosion heuristic:
    - `MAX_UPPER_BOUND_SIZE: usize = 4`
    - estimates cost from `union_size` and `intersection_size`
    - returns `IntersectionResult::CannotSimplify` before constructing a large distributed intersection
- `PathBounds::compute` uses `ConstraintBoundsBuilder` per typevar per BDD path.
- `PathBounds::default_solve` chooses solutions from finalized `ConstraintBounds`:
    - for bounded typevars, it prefers an explicit lower bound
    - otherwise it may return the upper bound intersected with the declared bound
    - for constrained typevars, it filters declared constraints using materialized lower/upper bounds
- `TypeVarSolution` currently stores `solution: Type<'db>`, so final inferred solutions must still be ordinary `Type`s unless a larger API change is made.

## Design summary

Introduce a new factored upper-bound representation for accumulated path/solution bounds while preserving simple per-constraint bounds.

## Decisions made

- Keep individual BDD constraints simple: `ConstraintBounds` should continue to store `upper: Option<Type<'db>>`.
    - Preserve the invariant that an individual constraint's upper bound is never a non-trivial intersection; direct constraint construction should keep splitting simple upper intersections into separate BDD constraints.
    - A constraint upper bound may still be a single union type, e.g. `T <= A | B`.
- Add an `UpperBound` type for accumulated path/solution upper bounds.
    - `UpperBound` should wrap the current CNF clause accumulator, changing the storage from `FxIndexSet<Type<'db>>` to `FxOrderSet<Type<'db>>` so the finished path-bound data can participate in `PathBounds`'s existing `Hash` derive.
    - An empty `UpperBound` represents no explicit upper clauses, semantically equivalent to `object`.
    - `UpperBound` stores raw upper clauses, including union clauses; it should not split into `Single`/`Intersection` variants.
    - `UpperBound` owns the method(s) that decide whether to exactly materialize the clauses or use bounded partial DNF witness extraction.
    - `ConstraintBoundsBuilder` should use `UpperBound` for its mutable `upper` accumulator; `add_upper` should delegate to `UpperBound`'s clause-addition/pruning logic.
    - `ConstraintBoundsBuilder::finish` should return a `PathBound`, moving its `UpperBound` into `PathBound.upper` without calling `IntersectionType::from_elements`.
- Change `PathBound` so it no longer stores `ConstraintBounds`.
    - Make `PathBound` a named struct, not a tuple alias.
    - `PathBound.bound_typevar` stores the typevar.
    - `PathBound.lower` remains `Option<Type<'db>>`, produced exactly as today by unioning lower clauses with `UnionType::from_elements`.
    - `PathBound.upper` is `UpperBound<'db>`.
    - This avoids inventing a separate accumulated-bounds wrapper type; the existing path-bound layer stores the accumulated lower and upper fields directly.
    - `PathBounds::solve_with` hooks should receive `PathBound` by reference while `PathBounds::solve_with` takes `&self`.
    - Since `PathBound` owns the bound typevar and bounds together, `PathBounds::solve_with` hooks should not receive the typevar as a separate parameter. Preserve the existing separate `variance` parameter by computing it once from the `PathBound` and passing `choose(variance, &path_bound)`.
    - Higher-level `SpecializationBuilder::build_with` hooks can keep receiving the typevar separately because they also handle unmapped typevars; change their bounds argument from `Option<ConstraintBounds>` to `Option<&PathBound>`.
    - For old-solver/hash-map fallback paths that only have an inferred `Type`, construct a temporary exact `PathBound` via `PathBound::exact` and pass `Some(&path_bound)` to the high-level hook, preserving current behavior for hooks that inspect lower bounds.
- `UpperBound` clause addition should preserve the current `ConstraintBoundsBuilder::add_upper` behavior: add/prune one ordinary `Type` clause at a time and do not split `Type::Intersection` inputs.
    - Direct constraint construction remains responsible for splitting non-trivial upper intersections before they reach path extraction.
    - Because ordinary `Type` is already in DNF, a `Type::Intersection` input will not contain nested union clauses that must be extracted.
    - Detect union-bearing upper bounds by checking for visible top-level `Type::Union` clauses in the stored clause set; do not resolve aliases or inspect recursively unless later evidence requires it.
    - If there are no union clauses, witness extraction should return exact materialization. Revisit using the bounded witness algorithm for the no-union case only if we can prove it produces the full exact result without a performance penalty.
    - If there are union clauses, bounded witness extraction should first exactly materialize the non-union clauses into the initial candidate (or `object` if there are no non-union clauses), then branch only over union clauses.
    - Default solving should not mutate the stored `PathBound` when considering a declared upper bound. Instead, pass the declared upper bound into witness extraction as an extra solving-time conjunctive clause for upper-only witness selection.
    - Keep `UpperBound::is_satisfied_by` and `UpperBound::when_satisfied_by` focused on accumulated upper bounds only; callers should separately check declared bounds/constraints when needed.
    - Do not treat constrained TypeVar constraints as union-like branching clauses. A constrained TypeVar must solve to exactly one of its declared constraint types; a union of constraints is not a valid solution, and neither is an arbitrary subtype of a constraint.
- `UpperBound` redundancy pruning should use `Type::is_redundant_with`, matching the existing `ConstraintBoundsBuilder::add_upper` behavior. This relation was created specifically for type simplification/redundancy checks.
    - Do not shrink `UpperBound` storage during incremental `add_clause` calls. Shrink once during `ConstraintBoundsBuilder::finish` before storing the `UpperBound` in cached `PathBounds`, using an explicit `UpperBound::shrink_to_fit(&mut self)` method.
- Keep the first implementation pass focused on path/solution bounds and the minimal sequent-map work needed to remove the old upper-bound size heuristic.
    - `ConstraintId::intersect` should remain a "simple-constraint derivation" helper, not a general factored-upper derivation helper.
    - Combine lower bounds and upper bounds logically, then always perform the combined range satisfiability check (`lower <= merged_upper`) before punting.
    - For merged upper bounds, use a transient `UpperBound` so satisfiability can be checked clause-wise without exact DNF materialization.
    - If the merged upper bound contains union clauses, return `Disjoint` when the combined range is unsatisfiable and `CannotSimplify` otherwise.
    - Only return `Simplified` when the merged range can be represented as one simple constraint; do not try to derive arbitrary factored upper-bound constraints from union-bearing upper intersections.
    - Do not redesign broader sequent-map implication logic in this iteration unless implementation reveals a direct correctness blocker.
- Do not design broad `UpperBound` accessor/helper APIs in advance.
    - Add accessors or helper methods only when required by load-bearing behavior during implementation.
    - Method names and signatures in this plan are illustrative where they support the top-level behavior; implementation should choose the narrowest helper shape needed by actual callers.
    - When an accessor is needed, choose its shape based on the immediate caller's semantic need rather than exposing storage details by default.
- Preserve current declared TypeVar bound/constraint validation semantics.
    - For declared upper bounds, keep the existing possible-assignability check against the declared bound's top materialization.
    - For constrained TypeVars, keep exact constrained-TypeVar solution semantics and update only the accumulated-upper side to use factored upper checks.
- Avoid hidden exact materialization of accumulated upper bounds.
    - `ConstraintBounds` may keep its current cheap `Option<Type>` materialization helpers for individual constraints where appropriate.
    - `UpperBound::materialize_exact` should be clearly documented as potentially expensive because it converts the CNF clauses to ty's ordinary DNF `Type` representation by calling `IntersectionType::from_elements` over the stored clause iterator.
    - It should take `&self`; consuming the `UpperBound` would not let the ordinary `Type` representation reuse the `FxOrderSet` storage anyway.
    - Add a guarded production helper for the common safe path, e.g. `materialize_exact_if_no_visible_unions`, which returns `None` if any stored clause is a visible top-level union and returns `Some(object)` for an empty upper bound.
    - The helper name/documentation should be precise: it does not resolve aliases, so hidden alias-expanded unions can still distribute during exact materialization. Revisit only if profiling/ecosystem results show this matters.
    - `bounded_partial_dnf_witness` should use the guarded no-visible-union materialization path before invoking bounded partial DNF when there is no declared upper. When a declared upper is present, it must operate over the combined iterator of stored clauses plus the declared upper so the guard/materialization/branching logic accounts for both.
    - Any display/debug output for path bounds should render factored upper bounds directly rather than materializing them into a single ordinary `Type` just for display.

Conceptually:

```text
UpperBound = object                         // no explicit upper clauses
           | U1                             // one ordinary Type clause
           | U1 & U2 & ... & Un             // factored conjunction of Type clauses
```

Each `Ui` remains an ordinary `Type<'db>`, and may itself be a union. The `UpperBound` as a whole is a CNF/factored-intersection representation. Combining upper bounds appends/prunes clauses instead of forcing ty's global DNF `Type` representation to distribute intersections across unions.

This representation should be used:

1. In accumulated path/solution bounds, so `ConstraintBoundsBuilder::finish` can preserve factored upper bounds instead of collapsing them to a single `Type`.
1. Transiently in `ConstraintId::intersect`, when merging two simple per-constraint upper bounds for satisfiability checks without exact DNF materialization.
1. At final solution extraction: produce a compact witness type using bounded partial DNF rather than either full CNF→DNF conversion or unconditional `Unknown`.

## Key semantic choices

### Lower bounds

Keep lower bounds as an ordinary `Option<Type<'db>>` for now. Lower bounds are unioned, and ty's DNF representation handles unions naturally. The current lower-bound behavior is not the source of the `jax` blowup.

### Upper bounds

Represent accumulated explicit upper bounds as a factored conjunction of ordinary `Type` clauses. Missing upper bound still means `object` and is represented by an empty clause set.

`UpperBound` should be a small wrapper around the current `ConstraintBoundsBuilder` upper-clause accumulator, using `FxOrderSet` instead of `FxIndexSet` so that path bounds can continue to derive `Hash`:

```rust
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct UpperBound<'db> {
    clauses: FxOrderSet<Type<'db>>,
}
```

Invariants to maintain:

- An empty `UpperBound` represents no explicit upper clauses; materializing it yields `object`.
- Each stored type is one CNF upper clause. Clauses may themselves be union types.
- No `object` clauses, since they are redundant; `UpperBound::add_clause` should explicitly elide `object` before redundancy pruning.
- A `Never` clause means the entire upper bound is `Never`; `UpperBound::add_clause` should explicitly collapse the stored clauses to exactly `[Never]`.
- If `UpperBound` is already `[Never]`, adding any later non-`Never` clause is a no-op.
- Add a comment around these `object`/`Never` fast paths noting that they are optimizations; the general redundancy-pruning loop should also handle them correctly.
- Duplicate clauses are removed.
- Redundant supertype clauses are removed using `Type::is_redundant_with`, matching the current `ConstraintBoundsBuilder::add_upper` behavior:
    - if existing clause `E` is narrower than new clause `N` (`E <= N`), then `N` adds nothing.
    - if new clause `N` is narrower than existing clause `E`, remove `E`.
- Clause order should be deterministic and stable enough to preserve reproducible output and hashing.

### Materialization and witness extraction

Add explicit APIs rather than implicitly materializing everywhere:

```rust
impl<'db> UpperBound<'db> {
    fn none() -> Self;
    fn from_clause(db: &'db dyn Db, clause: Type<'db>) -> Self;
    fn from_clauses(db: &'db dyn Db, clauses: impl IntoIterator<Item = Type<'db>>) -> Self;
    fn add_clause(&mut self, db: &'db dyn Db, clause: Type<'db>);

    fn is_empty(&self) -> bool;
    fn has_explicit_bound(&self) -> bool;

    /// Exact conversion to ordinary Type. This may be expensive and should be used sparingly.
    fn materialize_exact(&self, db: &'db dyn Db) -> Type<'db>;

    /// Exact conversion only when no stored clause is a visible top-level union.
    /// Does not resolve aliases, so alias-expanded hidden unions may still distribute.
    fn materialize_exact_if_no_visible_unions(&self, db: &'db dyn Db) -> Option<Type<'db>>;

    /// Returns a compact witness `W` such that `W <= self` and satisfies an optional declared
    /// upper bound, using bounded partial DNF when disjunctive clauses are present and
    /// exact/simple materialization when they are not.
    fn bounded_partial_dnf_witness(
        &self,
        db: &'db dyn Db,
        declared_upper: Option<Type<'db>>,
        budget: PartialDnfBudget,
    ) -> Option<Type<'db>>;
}
```

Avoid casually replacing accumulated upper-bound materialization with exact `UpperBound::materialize_exact` calls. Most checks involving accumulated/factored uppers should be rewritten to reason clause-by-clause or use the witness method when selecting a solution.

### Assignability to an upper bound

For a type `T` to satisfy the factored upper bound:

```text
T <= U1 & U2 & ... & Un
```

it is enough and necessary to check:

```text
T <= U1
T <= U2
...
T <= Un
```

Add load-bearing helpers such as:

```rust
impl<'db> UpperBound<'db> {
    fn is_satisfied_by(&self, db: &'db dyn Db, ty: Type<'db>) -> bool;

    fn when_satisfied_by<'c>(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        lower: Type<'db>,
    ) -> ConstraintSet<'db, 'c>;
}
```

Implement `is_satisfied_by` by checking `ty <= clause` for every stored clause, with ordinary short-circuiting and no exact upper-bound materialization. An empty upper bound is vacuously satisfied: `object` is the top type, and all types are by definition assignable to it. Callers that care about whether the upper bound was explicit should check `has_upper` separately.

`when_satisfied_by` is needed for sequent-map and satisfiability logic that currently derives constraint sets from `lower <= upper`. Return the conjunction of `lower <= clause` for every stored clause, again without exact upper-bound materialization. For an empty upper bound, return `ConstraintSet::always(builder)`. Each non-empty per-clause assignability check should use the owned/Salsa-cached path (`when_constraint_set_assignable_to_owned`) and then load the owned set into the caller's builder before combining; prefer Salsa reuse over the lingering builder-local call sites.

The exact names should match local style.

## Bounded partial DNF witness

For solution extraction, we are allowed to choose any type that satisfies the lower and upper bounds. Today, choosing the lower bound when present and the upper bound otherwise is only a heuristic.

When no lower bound is available, instead of fully converting a factored upper bound to DNF, compute a compact witness `W` such that:

```text
W <= UpperBound
```

This witness can be a single product term or a bounded union of product terms.

Given:

```text
UpperBound = C1 & C2 & ... & Cn
Ci = ai1 | ai2 | ... | aik
```

Full CNF→DNF computes all combinations:

```text
(a11 & a21 & ... & an1) | ...
```

But solution extraction only needs some satisfying type, not the largest exact upper type. A single non-bottom product term is a valid witness:

```text
a1j & a2k & ... & anm
```

because it is a subtype of every clause.

### Pseudocode

This should be a method on `UpperBound`:

```rust
impl<'db> UpperBound<'db> {
    /// Return a compact ordinary Type W such that W <= self and, if present,
    /// W <= declared_upper.
    ///
    /// This is an under-approximation of the full CNF upper bound. It may return only some
    /// satisfying DNF terms, not all of them.
    fn bounded_partial_dnf_witness(
        &self,
        db: &'db dyn Db,
        declared_upper: Option<Type<'db>>,
        budget: PartialDnfBudget,
    ) -> Option<Type<'db>> {
        let clauses = self.clauses().chain(declared_upper);

        if no_clause_is_a_visible_union(clauses.clone()) {
            return Some(IntersectionType::from_elements(db, clauses));
        }

        // Each candidate is one DNF product term known to satisfy all processed union clauses.
        // The non-union clauses are materialized exactly into the initial candidate; this does
        // not require distributing across a union.
        let initial = IntersectionType::from_elements(
            db,
            clauses.clone().filter(|clause| !clause.is_union()),
        );
        let mut frontier = CandidateSet::new(budget.max_terms);
        frontier.insert(db, Candidate::from_type(initial));

        for union_clause in clauses.filter_map(Type::as_union) {
            let mut next = CandidateSet::new(budget.max_terms);

            for candidate in frontier.iter() {
                for alt in union_clause.elements(db) {
                    let refined = candidate.intersect_with_term(db, *alt);
                    if refined.is_known_never(db) {
                        continue;
                    }
                    next.insert(db, refined);
                    if next.len() >= budget.max_terms {
                        break;
                    }
                }
                if next.len() >= budget.max_terms {
                    break;
                }
            }

            if next.is_empty() {
                return None;
            }

            frontier = next;
        }

        Some(UnionType::from_elements(
            db,
            frontier.iter().map(|candidate| candidate.to_type(db)),
        ))
    }
}
```

Budget shape:

```rust
struct PartialDnfBudget {
    /// Beam width and maximum final union size.
    max_terms: usize,
}
```

Candidate pruning should prefer broader and simpler terms:

```rust
impl CandidateSet {
    fn insert(&mut self, db: &'db dyn Db, new: Candidate<'db>) {
        let new_ty = new.to_type(db);

        // If an existing candidate is broader, the new narrower candidate adds nothing.
        if self.items.iter().any(|old| {
            new_ty.is_constraint_set_assignable_to(db, old.to_type(db))
        }) {
            return;
        }

        // Remove existing candidates made redundant by the new broader candidate.
        self.items.retain(|old| {
            !old.to_type(db).is_constraint_set_assignable_to(db, new_ty)
        });

        self.items.push(new);
    }

    fn truncate_to_budget(&mut self, db: &'db dyn Db) {
        self.items.sort_by_key(|candidate| {
            (
                candidate.complexity(db),
                candidate.source_order_key(),
            )
        });
        self.items.truncate(self.max_terms);
    }
}
```

The core invariant:

```text
After processing clause i, every candidate C satisfies:

C <= clause_0
C <= clause_1
...
C <= clause_i
```

Therefore, the final union of candidates satisfies the full upper bound, because each union element satisfies every CNF clause.

### Candidate representation

A candidate can initially be represented as a small list/set of positive intersection terms:

```rust
struct Candidate<'db> {
    terms: SmallVec<[Type<'db>; 4]>,
}
```

`Candidate::object()` has no terms. `candidate.intersect_with_term(db, alt)` appends/prunes a term. `candidate.to_type(db)` can call `IntersectionType::from_elements` over the candidate's terms.

This still performs ordinary intersection construction for a single product term, but avoids constructing the full cross-product union. If even a single product term becomes too expensive, add a candidate-local budget and treat that candidate as unusable.

### Fallback policy

If `bounded_partial_dnf_witness` cannot find a non-`Never` witness within budget:

- `Never` is always a subtype of any upper bound and is therefore sound, but may be overly narrow.
- `Unknown` honestly represents loss of precision and is gradual, but may not strictly satisfy upper bounds in the same semantic sense as an ordinary static subtype.
- Leaving the typevar unsolved may preserve existing fallback behavior in some call paths.

Initial recommendation:

1. Prefer a bounded witness when possible.
1. If no witness is found, use `Never` only if that does not cause bad downstream behavior in tests.
1. Otherwise fall back to `Unknown` or unsolved based on current solver expectations.

This policy should be decided by tests and real-world ecosystem behavior.

## Implementation phases

These phases are implementation slices, not a license to defer cleanup or testing. Keep display/debug output, comments, and local style updates adjacent to the code they describe. Tests should pass after each phase, or at least before each checkpoint that becomes a separate jj revision.

### Phase 0: Baseline and reproducer

- [ ] Confirm the current worktree is based on `dcreager/deferred-quantification`.
- [ ] Build ty in debug mode.
- [ ] Reproduce or at least baseline the `jax` regression with an explicit memory limit.
    - Do **not** run `jax` without a memory limit.
    - Record the exact command, memory limit, and observed behavior in this plan.
- [ ] Identify or create a smaller mdtest/unit test if feasible. Do not block the core implementation on perfect minimization.

### Phase 1: Introduce `UpperBound`

- [ ] Add an `UpperBound<'db>` type in or near `constraints.rs`, wrapping `FxOrderSet<Type<'db>>`.
- [ ] Document the factored-CNF representation and invariants next to the type.
- [ ] Implement constructors for no explicit upper bound, single clause, and multiple clauses.
- [ ] Implement explicit-bound checks such as `is_empty` / `has_explicit_bound`.
- [ ] Add `UpperBound` accessors only as required by load-bearing methods; avoid speculative general-purpose clause accessors.
- [ ] Implement `UpperBound::add_clause` as the shared accumulation primitive used both during path walking and when adding a declared upper bound during solution extraction.
- [ ] Keep `UpperBound::add_clause` equivalent to current `ConstraintBoundsBuilder::add_upper`: add/prune one `Type` clause and do not split `Type::Intersection` inputs.
- [ ] Explicitly elide `object` clauses at the start of `UpperBound::add_clause` before redundancy pruning.
- [ ] Explicitly collapse `UpperBound` to exactly `[Never]` when adding a `Never` clause.
- [ ] If `UpperBound` is already `[Never]`, make later non-`Never` additions a no-op.
- [ ] Comment that the `object`/`Never` fast paths are optimizations and that the general redundancy-pruning loop should also handle them correctly.
- [ ] Do not shrink `UpperBound` storage during incremental `add_clause` calls; add `UpperBound::shrink_to_fit(&mut self)` and call it before storing in `PathBounds`.
- [ ] Use `Type::is_redundant_with` for redundant upper-clause pruning, matching existing `ConstraintBoundsBuilder::add_upper` behavior.
- [ ] Implement exact materialization behind an explicitly named method, using `IntersectionType::from_elements` over the stored clauses only at that explicit boundary.
- [ ] Implement a guarded production helper such as `UpperBound::materialize_exact_if_no_visible_unions`, returning `None` when any stored clause is a visible top-level union and `Some(Type::object())` for an empty upper bound.
    - The helper should not resolve aliases; document that hidden alias-expanded unions can still distribute during exact materialization.
- [ ] Implement union-clause detection using visible top-level `Type::Union` clauses in the stored clause set, relying on ordinary `Type` DNF invariants.
- [ ] In `UpperBound::bounded_partial_dnf_witness`, account for an optional declared upper bound without mutating the stored `PathBound`: chain the declared upper bound into the conjunctive clauses.
- [ ] Do not pass constrained TypeVar constraints into `bounded_partial_dnf_witness` as union-like branching clauses; constrained TypeVars require separate exact-constraint selection logic.
- [ ] In `UpperBound::bounded_partial_dnf_witness`, account for both stored clauses and the optional declared upper when deciding whether exact materialization is safe and when branching over visible union clauses.
    - The guarded helper can be used directly only when there is no declared upper, or generalized internally to accept an extra clause iterator.
- [ ] For union-bearing upper bounds, initialize bounded witness search with exact materialization of all non-union conjunctive clauses, then branch only over visible top-level union clauses.
- [ ] Add unit tests for `UpperBound` construction, pruning, `object`, `Never`, duplicate clauses, deterministic ordering, exact materialization, and union-clause witness behavior.

### Phase 2: Change `PathBound` to store accumulated bounds directly

- [ ] Keep `ConstraintBounds<'db>` as the simple per-constraint representation with `upper: Option<Type<'db>>`.
- [ ] Preserve the existing direct-construction invariant that simple upper intersections are split into separate BDD constraints.
- [ ] Replace the `PathBound` alias `(BoundTypeVarInstance<'db>, ConstraintBounds<'db>)` with a named `PathBound` struct that stores `bound_typevar`, `lower: Option<Type<'db>>`, and `upper: UpperBound<'db>` directly.
- [ ] Keep lower-bound finalization unchanged: `PathBound.lower` is still an `Option<Type<'db>>` produced by exact `UnionType::from_elements` over accumulated lower clauses.
- [ ] Preserve old semantics for missing explicit upper bounds (`object`) via an empty `UpperBound`.
- [ ] Implement `PathBound` helpers named `variance`, `lower_or_never`, and `has_upper` as needed by solution extraction.
- [ ] Add `PathBound::exact(db, bound_typevar, ty)` to mimic the old `ConstraintBounds::exact` constructor for old-solver/hash-map fallback paths.
- [ ] Do not add hidden exact upper materialization helpers to `PathBound`; expose exact upper materialization only as an explicit `UpperBound::materialize_exact` operation, document that it may distribute union clauses, and use it sparingly.
- [ ] Update solution/path APIs that currently expose `ConstraintBounds` from `PathBounds` to expose `PathBound` instead.
- [ ] Update `PathBounds::solve_with` and related choose hooks to pass `choose(variance, &path_bound)` while `solve_with` takes `&self`, avoiding hidden clones and avoiding a redundant separate typevar parameter.
- [ ] Preserve the existing separate `variance` parameter in `PathBounds::solve_with` hooks, computed once from the `PathBound`.
- [ ] Update higher-level `SpecializationBuilder::build_with` hooks from `Option<ConstraintBounds>` to `Option<&PathBound>`, while keeping the separate typevar parameter for the unmapped-`None` case.
- [ ] In `solve_hash_map_with` and other old-solver fallback paths that only have an inferred `Type`, construct a temporary `PathBound::exact(...)` and pass `Some(&path_bound)` to the high-level hook.
- [ ] Keep individual constraint construction APIs (`ConstraintId::new_with_bounds`, `Constraint::new_node_with_bounds`, `ConstraintSet::constrain_typevar_with_bounds`) based on optional ordinary upper `Type`s.

### Phase 3: Minimal sequent-map construction changes

- [ ] Rewrite `ConstraintId::intersect` as a simple-constraint derivation helper:
    - merge optional ordinary upper `Type`s into a transient `UpperBound` only for reasoning;
    - always perform the combined range satisfiability check (`lower <= merged_upper`) before punting;
    - if the transient merged upper is union-bearing, return `Disjoint` if the range is unsatisfiable and `CannotSimplify` otherwise;
    - only return `Simplified` when the merged range can be represented as one simple per-constraint `ConstraintBounds` value.
- [ ] Remove the existing `MAX_UPPER_BOUND_SIZE` heuristic once union-bearing uppers are handled by the `Disjoint`/`CannotSimplify` branch.
- [ ] Remove or update obsolete comments about the sequent-map heuristic in the same change that removes the heuristic.
- [ ] Rework satisfiability checks from `lower <= materialized_upper` to `UpperBound::when_satisfied_by` or clause-wise boolean checks as appropriate.
- [ ] Leave deeper sequent-map implication logic intact unless implementation reveals a direct correctness blocker for the factored path-bound change.
- [ ] Audit sequent-map helpers that substitute or derive upper bounds only as needed to confirm the first-pass scope:
    - `add_mutual_sequents_for_different_typevars`
    - `add_mutual_sequents_for_same_typevars`
    - `add_nested_typevar_sequents`
    - `add_concrete_sequents`
- [ ] Add tests that exercise combining constraints with large union upper clauses without hitting DNF explosion.

### Phase 4: Preserve factored upper bounds through path extraction

- [ ] Change `ConstraintBoundsBuilder`'s upper accumulator from a raw `FxIndexSet<Type<'db>>` to `UpperBound<'db>`, with `add_upper` delegating to `UpperBound::add_clause`.
- [ ] Change `ConstraintBoundsBuilder::finish` so it returns a `PathBound`, moving the accumulated `UpperBound` directly into `PathBound.upper` instead of exact materialization or the current `Unknown` fallback heuristic.
- [ ] Call `upper.shrink_to_fit()` directly in `ConstraintBoundsBuilder::finish` before constructing the `PathBound`; do not add a `PathBound::shrink_to_fit` helper unless future fields need it.
- [ ] Ensure `PathBounds::compute` still produces stable results.
- [ ] Update path-bound display/debug code at the same time as the `PathBound` representation change, rendering factored upper bounds clearly and directly without materializing `UpperBound` into a single ordinary `Type`.
- [ ] Audit the touched display/debug helpers for accidental exact materialization.
- [ ] Update `PathBounds` snapshots/tests if displays change.

### Phase 5: Add bounded partial DNF witness generation

- [ ] Implement `UpperBound::bounded_partial_dnf_witness`.
- [ ] Document near the implementation that final witness extraction is an under-approximation used to produce a compact satisfying solution, not an exact materialization of the full factored upper bound.
- [ ] Implement `PartialDnfBudget` with conservative compile-time constants, starting with `MAX_PARTIAL_DNF_TERMS: usize = 4`; do not add runtime configuration.
- [ ] Do not add a separate `max_alternatives_per_clause` / scan-cap budget initially; use `max_terms` only, and add a scan cap later only if profiling shows it is necessary.
- [ ] Implement candidate representation and deterministic insertion-order pruning.
    - Use redundancy pruning when inserting candidates: discard a new candidate if an existing broader candidate already covers it; remove existing candidates covered by a new broader candidate.
    - Do not add explicit complexity ranking initially; rely on deterministic union-alternative scan order plus redundancy pruning.
- [ ] Keep up to `max_terms` candidate product terms and return `UnionType::from_elements` over the final frontier rather than stopping at the first valid witness.
- [ ] When branching on a union clause, scan alternatives in deterministic order and stop once the next frontier reaches `max_terms`; continue past rejected/`Never` refinements until either the frontier is full or the union is exhausted.
- [ ] Ensure every returned witness satisfies all upper clauses.
- [ ] Add focused tests:
    - single-clause upper bound returns that clause or a compact equivalent
    - `(A | B) & (B | C)` can choose `B` when appropriate
    - `(A | B) & (C | D)` chooses a compact non-bottom product term if one exists
    - large cross-product cases stay within budget
    - `Never` clause behavior
    - fallback behavior when no candidate survives within budget

### Phase 6: Update solution extraction

- [ ] In `PathBounds::default_solve`, when a lower bound exists, keep preferring the lower bound, but first validate `lower <= accumulated_upper` and separately validate declared typevar bounds/constraints using the current declared-bound semantics; invalidate the path with `Err(())` if the chosen lower witness does not satisfy all required bounds.
- [ ] For bounded typevars with no lower bound, call `bounded_partial_dnf_witness` with the declared upper bound only when there is an explicit accumulated upper or the declared bound is not the default unconstrained `object`; avoid inferring `object` solely from the absence of bounds.
- [ ] For constrained typevars, preserve exact-constraint selection semantics: zero compatible declared constraints invalidates the path, exactly one compatible declared constraint selects that exact constraint type, and multiple compatible constraints leave the typevar unsolved. Do not return a union of declared constraints.
    - Preserve current compatibility semantics: a lower bound such as `bool` can make declared constraint `int` compatible, but the selected solution is the exact declared constraint `int`, not `bool`.
    - Replace the old exact upper materialization check with `upper.when_satisfied_by(db, builder, constraint.top_materialization(db))` / equivalent clause-wise logic for `top_materialization(constraint) <= accumulated_upper`.
- [ ] Implement `UpperBound::is_satisfied_by` as a clause-wise check: `ty <= each stored upper clause`; an empty upper bound is vacuously satisfied.
- [ ] Implement `UpperBound::when_satisfied_by` for paths that need the constraint set for `lower <= upper_bound`; return `always` for an empty upper bound, otherwise combine `lower <= each stored upper clause` with conjunction.
    - Use `when_constraint_set_assignable_to_owned` for each clause and load the owned result into the caller's builder before combining, rather than using builder-local assignability directly.
- [ ] Make `UpperBound::bounded_partial_dnf_witness` return `Option<Type<'db>>`; `None` means no compact non-`Never` witness was found, and solver code decides the fallback policy.
- [ ] If default solving needs an upper-only solution and bounded witness generation returns `None`, leave the typevar unsolved for that path with `Ok(None)` rather than inferring `Never`, `Unknown`, or invalidating the path.
- [ ] Ensure `TypeVarSolution` can remain `solution: Type<'db>` for this iteration.

## Checkpoint validation (not a separate phase)

Perform validation whenever a phase is completed, and before each checkpoint that becomes a separate jj revision:

- [ ] Check for local style issues while editing, including opportunities to use let-chains.
- [ ] Run focused Rust tests for the code changed in the checkpoint.
- [ ] Run affected mdtests, especially generics/call/protocol/overload tests when solution behavior changes.
- [ ] Keep tests passing before checkpointing; do not defer broken tests to a later cleanup phase.
- [ ] Review any added or updated snapshots.
- [ ] Check for `.pending-snap` files if inline snapshots are affected.
- [ ] Run `/home/dcreager/bin/jpk` or the repo-prescribed prek command on changed files before handoff.

Before final handoff, run the `ty_python_semantic` test suite:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
CARGO_PROFILE_DEV_DEBUG="line-tables-only" \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic
```

Also run a memory-limited `jax` ecosystem check before final handoff and compare against the baseline. Do **not** run `jax` without a memory limit.

## Open design questions

- [x] Should individual per-constraint `ConstraintBounds` store `UpperBound`? No. Keep `ConstraintBounds.upper: Option<Type<'db>>`; `UpperBound` is for accumulated path/solution uppers and transient merged-upper reasoning.
- [x] What should `UpperBound` store? Wrap a CNF clause set: `FxOrderSet<Type<'db>>`, preserving the current `ConstraintBoundsBuilder::upper` semantics while supporting `PathBounds`'s existing `Hash` derive.
- [x] Should `UpperBound::add_clause` split `Type::Intersection` inputs? No. Preserve current `ConstraintBoundsBuilder::add_upper` behavior; ordinary `Type` is already DNF, so intersections do not contain nested unions that need extraction.
- [x] How should `UpperBound` detect whether it has union clauses? Check for visible top-level `Type::Union` clauses in the stored set; do not resolve aliases or inspect recursively unless later evidence requires it.
- [x] What should `UpperBound::bounded_partial_dnf_witness` do when there are no union clauses? Return exact materialization. Revisit only if the bounded algorithm can be proven to return the full exact result without a performance penalty.
- [x] For bounded partial DNF, should non-union clauses be processed as an initial fixed candidate? Yes. Materialize all non-union clauses exactly into the initial candidate, and branch only over union clauses.
- [x] When branching on a union clause, how many alternatives should bounded partial DNF consider per current candidate? Scan alternatives in deterministic order and stop once the next frontier reaches `max_terms`, while continuing past rejected/`Never` refinements until the frontier is full or the union is exhausted.
- [x] Should bounded partial DNF return one witness or a bounded union of witnesses? Keep up to `max_terms` product-term candidates and return their union.
- [x] Should `PathBound` store `ConstraintBounds`? No. Change `PathBound` so it stores the bound typevar plus `lower: Option<Type<'db>>` and `upper: UpperBound<'db>` directly.
- [x] Should `ConstraintBoundsBuilder::finish` materialize the upper bound? No. It should return a `PathBound` and move the accumulated `UpperBound` into `PathBound.upper`.
- [x] What exact relation helper should define upper-clause redundancy? Use `Type::is_redundant_with`, matching current `ConstraintBoundsBuilder::add_upper` behavior.
- [x] Do we need an `UpperBound::when_satisfied_by` helper that returns a `ConstraintSet`? Yes. Sequent-map and satisfiability logic need constraint sets for `lower <= upper_bound`; implement this clause-wise and combine with conjunction.
- [x] Should `UpperBound::when_satisfied_by` use builder-local per-clause assignability or owned/Salsa-cached per-clause assignability? Use owned/Salsa-cached assignability and load each owned result into the caller's builder; expected Salsa reuse should dominate.
- [x] What should `UpperBound::when_satisfied_by` return for an empty upper bound? Return `ConstraintSet::always(builder)`; callers can separately check for a missing/empty upper bound when needed.
- [x] What should `UpperBound::is_satisfied_by` return for an empty upper bound? Return `true`; `object` is the top type, so all types are assignable to it by definition.
- [x] Should `UpperBound::add_clause` elide `object` clauses explicitly or via redundancy pruning? Explicitly elide `object` before redundancy pruning.
- [x] Should `UpperBound::add_clause` collapse to exactly `[Never]` when adding `Never`? Yes.
- [x] If `UpperBound` is already `[Never]`, should later non-`Never` additions be a no-op? Yes. Add comments explaining that the `object`/`Never` fast paths are optimizations and the general redundancy-pruning loop should also handle them correctly.
- [x] Should `UpperBound::add_clause` shrink storage after removals? No. Avoid shrink/grow churn during accumulation; shrink once during `ConstraintBoundsBuilder::finish` before storing in cached `PathBounds`.
- [x] Should `UpperBound`'s final shrink method consume `self` or mutate `&mut self`? Use an explicit `shrink_to_fit(&mut self)` method; a consuming finish method would still rely on callers remembering to invoke it.
- [x] Should `PathBound` have a shrink helper, or should `ConstraintBoundsBuilder::finish` call `upper.shrink_to_fit()` directly? Call `upper.shrink_to_fit()` directly before constructing `PathBound`; add a `PathBound` helper only if future fields need it.
- [x] Should `PathBounds::solve_with` pass path bounds to the choose hook by value or by reference? By reference while `solve_with` takes `&self`; if `solve_with` later takes owned `self`, the hook can take owned bounds too.
- [x] What should the `solve_with` choose hook receive now that `PathBound` contains the bound typevar? Pass `choose(variance, &path_bound)`: keep variance separate, but do not pass a redundant separate typevar parameter.
- [x] What should higher-level `SpecializationBuilder::build_with` hooks receive? Keep the separate typevar parameter for the unmapped case, but change bounds from `Option<ConstraintBounds>` to `Option<&PathBound>`; this API should eventually go away with the old solver.
- [x] What should old-solver/hash-map fallback paths pass to high-level choose hooks? Construct a temporary exact `PathBound` using a `PathBound::exact` helper and pass `Some(&path_bound)`.
- [x] Should default solving mutate `PathBound.upper` when adding a declared typevar bound? No. Pass a declared upper bound into witness extraction as an extra solving-time conjunctive clause.
- [x] Can constrained TypeVar constraints be treated as union-like branching clauses for witness extraction? No. A constrained TypeVar must solve to exactly one declared constraint type; a union of constraints is not a valid solution, nor is an arbitrary subtype of a constraint.
- [x] Should default solving explicitly validate a chosen lower-bound solution against the accumulated upper bound? Yes. Before returning a lower-bound witness, check `lower <= accumulated_upper` clause-wise; return `Err(())` for the path if it fails.
- [x] What should low-level constraint-construction APIs accept? Keep low-level individual constraint construction based on optional ordinary upper `Type`s; `UpperBound` is for accumulated/path bounds and transient merged-upper reasoning.
- [x] Should direct constraint construction still split simple upper intersections into separate BDD constraints? Yes. Preserve the existing invariant.
- [x] Should `ConstraintId::intersect` derive factored upper-bound constraints? No. Keep it as a simple-constraint derivation helper: after combined satisfiability, return `CannotSimplify` for union-bearing merged uppers instead of constructing a factored derived constraint.
- [x] What are safe default budgets for bounded partial DNF? Use compile-time constants, initially `MAX_PARTIAL_DNF_TERMS: usize = 4`; do not add runtime configuration.
- [x] Do we need a separate `max_alternatives_per_clause` or scan-cap budget? No, not initially. Use only `max_terms`; add a scan cap later only if profiling shows it is necessary.
- [x] What should `bounded_partial_dnf_witness` return when no compact witness is found? Return `None`; the caller decides fallback policy.
- [x] What fallback should default solving use when bounded witness generation returns `None`? Leave the typevar unsolved for that path with `Ok(None)`.
- [x] Should constrained TypeVars keep the current zero/one/multiple compatible-constraint behavior, or should constraints be handled as union-like branching in bounded witness extraction? Keep exact constrained-TypeVar selection semantics: zero compatible constraints invalidates the path, one selects that exact constraint type, multiple leaves unsolved; never infer a union of constraints.
- [x] How should constrained-TypeVar compatibility be checked with factored uppers? Preserve current compatibility semantics: check `lower <= bottom_materialization(C)` and `top_materialization(C) <= accumulated_upper`, using `UpperBound::when_satisfied_by` / clause-wise logic for the upper check; if compatible, choose the exact declared constraint `C`.
- [x] Should `UpperBound::materialize_exact` consume `self` or borrow? Borrow `&self`; the ordinary `Type` representation cannot reuse the `FxOrderSet` storage anyway.
- [x] Should `PathBound` be a tuple alias or named struct? Use a named struct with `bound_typevar`, `lower`, and `upper` fields.
- [x] Should `PathBound::lower` continue to be a single `Option<Type>` produced by unioning lower clauses exactly? Yes. Keep current lower-bound behavior; lower unions are not the source of this regression.
- [x] What helper names should `PathBound` expose? Use `variance`, `lower_or_never`, and `has_upper`.
- [x] How aggressively should candidate ranking prefer broader terms versus simpler terms? Initially use deterministic insertion order plus redundancy pruning only; do not add explicit complexity ranking unless tests/ecosystem results show poor witnesses.
- [x] Should exact materialization be available only in tests/debug paths to prevent accidental regressions? No. Keep a clearly documented `materialize_exact` for rare/test/debug use, and add a guarded production helper such as `materialize_exact_if_no_visible_unions`; bounded witness extraction should use the guarded helper first.
- [x] Should `materialize_exact_if_no_visible_unions` return `Some(object)` for an empty upper bound? Yes. Empty means no explicit upper clauses and materializes exactly to `object`; callers can use `has_upper` when they need to distinguish explicitness.
- [x] Should the guarded exact materialization helper detect hidden alias-expanded unions? No. It should guard only visible top-level unions and be named/documented accordingly; revisit if profiling/ecosystem results show hidden alias expansion matters.
- [x] Should `bounded_partial_dnf_witness` include an optional declared upper in its visible-union guard/materialization/branching logic? Yes. It must operate over stored clauses plus the declared upper; using the guarded helper directly is only correct when there is no declared upper, unless the helper is generalized.
- [x] Should `UpperBound::is_satisfied_by` / `when_satisfied_by` accept an optional declared upper clause too? No. Keep them focused on accumulated uppers; default solving should check declared bounds/constraints separately except for upper-only witness extraction.
- [x] What checks should default solving use for declared TypeVar bounds/constraints? Preserve current semantics: declared upper bounds use possible assignability to the top materialization; constrained TypeVars keep exact constraint selection semantics, with only accumulated-upper checks rewritten to use factored uppers.
- [x] How much helper-method API should be decided in advance? Do not decide in-the-weeds helper APIs prematurely; focus the plan on top-level behavior and add narrow helpers only as needed during implementation.
- [x] What is the first-pass implementation scope? Limit it to path/solution bounds plus the minimal `ConstraintId::intersect` rewrite needed to remove the old size heuristic; do not attempt a broader sequent-map redesign unless a direct correctness blocker appears.
- [x] Should default solving call upper-only witness extraction when there is no explicit accumulated upper and the declared bound is `object`? No. Return `Ok(None)` to preserve current behavior and avoid inferring `object` solely from absence of bounds.

## Non-goals for this iteration

- Do not rewrite ty's global `Type` representation away from DNF.
- Do not add a new public `Type::FactoredIntersection` unless the bounded-witness approach proves insufficient.
- Do not treat the current solution-generation copy of the sequent-map explosion heuristic as the primary fix.
- Do not redesign broader sequent-map implication logic in the first pass unless required by a direct correctness blocker.
- Do not run `jax` without a memory limit.
