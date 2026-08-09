# Move declared type-variable restrictions into constraint-set domains

## Goal

Move enforcement of declared type-variable upper bounds and value constraints out of
`PathBounds::default_solve` and into the constraint set traversed to produce solutions. Represent
declared alternatives as actual TDD paths, distinguish validity restrictions from inference
evidence independently on each lower and upper bound, and preserve existing user-visible generic
inference and diagnostics.

Minimal diff churn is an explicit requirement:

- Keep the existing `ConstraintSet::node` field and Boolean operations.
- Do not add a stored domain field, rename `node`, add witness flags to `InteriorNodeData`, or
    track witness-constraint sets on solutions.
- Reuse root support, existing constraint/path representations, source ordering, and existing
    specialization-error construction.
- Do not attempt general correlation-preserving solution combination.
- Do not change eager existential or universal quantification; it is being replaced in a separate
    workstream.
- Do not change Python's general gradual-type assignability or declaration semantics. A dedicated
    static-sequent phase before Phase 5 owns the narrower logical correction: concrete proofs must
    use fully static bounds and subtyping, while structurally valid symbolic rules may propagate an
    existing gradual witness. Phase 5 then activates domains and replaces legacy declaration-aware
    solving; a potential Phase 6 restores accepted compatibility regressions. Do not choose a
    relation based on validity/evidence provenance.

## Workflow and handoff requirements

This document is the ground truth for phase order, dependencies, agreed decisions, and completion
status. Complete phases in order. An agent resuming the work must inspect the source code, tests,
and Jujutsu revisions to verify that completion markers are accurate before continuing.

Each implementation phase must be its own Jujutsu revision. Before editing for a new phase, create
a revision with `jj new -A @` and describe it with `jj describe`; descriptions must begin with
`[π]`. Never edit an existing revision with `jj edit`. Update this legacy in-repository plan only
after the phase passes full validation, using a separate plan-only revision.

Documentation and tests are cross-cutting requirements of every phase, not separate phases.
Prioritize focused tests of user-visible typing behavior. Existing mdtests using
`ty_extensions._internal.ConstraintSet` can describe implementation rather than user requirements;
change those tests when necessary to obtain the correct user-visible behavior. Do not add large
batteries of implementation-locking tests when existing coverage or a targeted invariant test
suffices.

The full test suite must pass at the end of every phase. If a subsequent phase is required for the
ultimately correct result, do not disable, comment out, or ignore tests. Instead, update the
assertion or mdtest expectation to the currently observed behavior and add a clear TODO describing
the expected correct behavior. Prefer sequencing that avoids temporary regressions entirely.

The static-sequent phase was initially the only deliberate exception where temporary user-visible
regressions were expected. Phase 5 may also retain explicitly characterized behavioral regressions
behind adjacent `XXX` expectations, but it must complete the structural migration: declared domains
supply the legal paths, pruning compares those paths, and `PathBounds::default_solve` must no longer
re-read declarations or reconstruct their alternatives. Both revisions must pass the full suite,
clippy, and prek. A potential Phase 6 may restore or explicitly resolve the accepted compatibility
and performance debt before selecting a landing tip.

Use `jj` for source control and inspect changes with `jj diff --git` or `jj diff --stat`. Run prek
in this Jujutsu workspace through `/home/dcreager/bin/jpk`. Never modify snapshot files or inline
snapshot bodies manually: regenerate them with the documented test commands, inspect every
snapshot change, and check for `.pending-snap` files when inline snapshots are involved.

## Staged integration: fully static sequents before Phase 5

Fully static sequent handling and domain activation remain one merge unit, but they are implemented
as test-clean project revisions. The static revision establishes the sound logical boundary and
records temporary compatibility regressions. The Phase 5 activation checkpoint activates domains
and records the still-observed behavior as passing `XXX` expectations. Phase 5 remains incomplete
until structural path-only solving replaces the legacy declaration-aware solver; a potential Phase
6 may then remove the accepted behavioral debt before the stack can land.

Investigation established this split:

- The sequent map treats gradual assignability as transitive logical implication. Exact validity
    alternatives such as `T = Any` and `T = int` can therefore become mutually implicative and be
    collected into one aggregate path.
- A prototype static-sequent gate on the Phase 5 diagnostic stack preserved the expected `int`
    result for the typed-dictionary union ordering regression and passed the full constraint-set
    ordering mdtest after cycle-safe derived-candidate validation.
- The same prototype exposed a provenance bug that could pollute a constrained TypeVar's declared
    validity equality with derived bounds. Every complete constrained-TypeVar domain path must
    retain exactly one pure validity equality; derived constraints belong in `Mixed`, and consumers
    must assert this invariant rather than accepting or skipping an invalid path.
- Removing gradual closure exposes independent alternative solutions. In particular, unbounded
    `Container[T]` inference selects `object` rather than preserving `Any`. The static phase records
    that intermediate result with an `XXX`; this accepted behavioral regression may be restored in
    a potential Phase 6 because declared-domain pruning alone cannot repair an unbounded variable.

Do not revive either rejected workaround from the standalone prototype: selectively retaining TDD
uncertain branches during abstraction, or locally unioning gradual solutions in `generics.rs`.
Likewise, do not add a provenance-dependent typing relation, an equality-only exception, broad
witness metadata, or a general correlation-preserving solution representation without another
approved replan.

The incomplete Phase 5 revision `pvynqtrq` and its pause revision are diagnostic snapshots, not
part of the implementation path. The new static-sequent phase starts after `sqkowvys` (the
completed Phase 4 stack plus the raw-graph fix) and the approved plan-only revisions. Phase 5 is a
child of that static revision. The former standalone plan is archived at
`~/.pi/memory/archive/plans/ruff/fully-static-sequents/PLAN.md` for historical rationale only;
this document now owns the active static requirements, phase order, and completion markers.

The implementation and landing order is:

1. completed domain-solving Phases 1–4 and the raw-graph fix;
1. a full-suite-clean but intentionally non-mergeable static-sequent revision;
1. the full-suite-clean Phase 5 domain-activation checkpoint, including enforcement of the
    validity-equality invariant;
1. Phase 5 structural completion, replacing legacy declaration-aware default solving;
1. potential Phase 6 compatibility restoration and performance work, which produces the first
    landable tip;
1. `dcreager/remove-remove-noninferable-2`.

## Current baseline

The principal implementation is `crates/ty_python_semantic/src/types/constraints.rs`.

A prerequisite revision has already removed the vestigial `satisfied_by_all_typevars` API and its
implementation-focused mdtests. Its removal also deleted `valid_specializations` and
`required_specializations`. Do not restore `satisfied_by_all_typevars` or bend the new
implementation around its former behavior. Reintroduce only the narrow domain-construction helper
needed for this project, using its historical implementation as a reference while preserving
existing user-visible gradual-bound and gradual-constraint behavior.

### Current per-path solving

`PathBounds::default_solve` currently inspects
`bound_typevar.typevar(db).require_bound_or_constraints(db, env)` for each type variable on each
TDD path.

For declared upper bounds, it:

- Uses the upper bound's top materialization; unbounded type variables effectively use `object`.
- Prefers an inferred lower bound after checking the path's upper bounds and the declaration.
- Otherwise intersects explicitly inferred upper bounds with the declared bound.
- Leaves a variable unsolved when there is no inference evidence.

For declared value constraints, it:

- Checks each declared alternative against the path's inferred bounds.
- Uses bottom/top materializations when checking gradual alternatives.
- Rejects paths that match no alternative.
- Promotes an inferred subtype to the declared alternative.
- Prefers the narrowest compatible alternative for lower-bound evidence and the broadest
    compatible alternative for upper-bound-only evidence.
- Prefers static over equivalent gradual alternatives, then preserves declaration order.
- Preserves gradual argument evidence when several alternatives are compatible.
- Preserves witnessed relationships to other type variables instead of always replacing them with
    concrete declared alternatives.

Because this logic runs independently on each path, it cannot compare declared alternatives that
appear on different TDD paths.

### Current support and type-variable arenas

`crates/ty_python_semantic/src/types/constraints/support.rs` records all type variables mentioned
in an individual constraint, including variables nested in lower or upper bounds. A root node's
support is the union of the supports reachable through its condition and all three TDD branches.

Support members are stable builder-local `TypeVarId`s. Builder-local and compacted owned arenas
currently map those IDs to `BoundTypeVarIdentity`, which deliberately omits declared bounds and
constraints. Domain construction instead needs a `BoundTypeVarInstance`.

Treat identities and retained instances as effectively one-to-one here. Multiple instances can
share one identity when lazy declarations have been resolved differently; preserve one instance
per identity and continue using the identity for interning, stable ordering, and membership.

### Current bounds and solutions

`ConstraintBounds` currently stores:

```rust
pub(crate) lower: Option<Type<'db>>,
pub(crate) upper: Option<Type<'db>>,
```

`None` means the relevant side is absent. Logical defaults are `Never` for the lower side and
`object` for the upper side, but an explicit `Never` or `object` must remain distinguishable from
an absent bound.

`ConstraintBoundsBuilder` currently accumulates all lower bounds into a union and upper bounds
into a factored `UpperBound` conjunction. It also classifies every accumulated bound as inference
evidence. `PathBound::variance` currently infers variance from the presence of accumulated lower
and upper bounds:

- Evidence on the lower side only: `TypeVarVariance::Contravariant`.
- Evidence on the upper side only: `TypeVarVariance::Covariant`.
- Evidence on both sides: `TypeVarVariance::Invariant`.
- No evidence: `TypeVarVariance::Bivariant`.

`PathBounds::compute` serves both `ConstraintSet::solutions_with` and the Salsa-cached
`Type::assignable_solutions_with_inferable` query. Its
`compute_simple_bound_conjunction` fast path avoids `PathAssignments` and sequent-map construction
for conjunctions of concrete bounds.

`Solutions::Constrained` holds a `Vec<Solution>`, and each `Solution` is a
`Vec<TypeVarSolution>`. One complete solution represents a correlated assignment. Existing
consumers, notably `SpecializationBuilder::solve_pending_with` in
`crates/ty_python_semantic/src/types/generics.rs`, often combine bindings independently; replacing
that broader behavior is outside this project.

## Agreed design

### Compute domains from root support

Do not store a domain alongside `ConstraintSet::node`. Instead compute it from root support when
extracting solutions:

```text
domain(node) = AND(valid_specializations(T) for T in support(node))
node_to_solve = node AND domain(node)
```

Reintroduce a narrow helper analogous to the removed `valid_specializations`:

- Unbounded variables and `P.args`/`P.kwargs` contribute `ALWAYS_TRUE`.
- An upper-bounded variable contributes a validity upper bound.
- A constrained variable contributes an OR of exact validity constraints, one for each declared
    alternative.
- Preserve gradual declared constraints directly instead of replacing them with their bottom/top
    materializations, either while constructing the domain or while computing effective path
    bounds.

For example:

```text
declared constraint: list[Any]
domain constraint:   list[Any] <= T <= list[Any]
```

Directly retaining the declared gradual type avoids additional domain-branch metadata and lets
solution selection recover the original declared alternative naturally. In particular, `T = Any`
must remain an exact validity equality; do not reinterpret it as `Never <= T <= object`, even
internally. The removed `satisfied_by_all_typevars` API and its historical `valid_specializations`
materialization strategy no longer constrain this design.

Include every type variable present in root support, not just inferable variables. Do not add a
separate recursive/fixed-point expansion: the agreed invariant is that relevant variable closure
is already represented by support.

Root support already follows existing Boolean operations and their short-circuiting. Keep those
operations unchanged. Preserve source-order sidecars and the distinction between missing and
explicit logical-default bounds.

### Track validity and evidence separately on each bound

Replace each `Option<Type>` inside `ConstraintBounds` with a provenance-aware enum:

```rust
enum ConstraintBound<'db> {
    Validity(Type<'db>),
    Evidence(Type<'db>),
}
```

`Validity` restricts which specializations are legal; `Evidence` additionally represents an
inference preference supplied by the original relationship. A missing lower bound is represented as
`Validity(Never)`, and a missing upper bound as `Validity(object)`. This preserves the important
distinction from explicitly inferred logical defaults, which are represented as `Evidence(Never)`
or `Evidence(object)`. Existing behavior for absent bounds therefore validates the same
validity/evidence combination rules used for nontrivial declaration restrictions. Record the
distinction independently for the lower and upper sides:

```text
argument evidence: Evidence(bool) <= T
validity domain:                      T <= Validity(int)
combined bounds:   Evidence(bool) <= T <= Validity(int)
```

The effective solution must satisfy both sides, but variance remains lower-bound-only because the
upper side contributes validity rather than evidence.

Similarly, an exact declared constraint can provide validity on both sides without manufacturing
invariant argument evidence:

```text
argument evidence: Evidence(bool) <= T
declared domain:   Validity(int) <= T <= Validity(int)
```

When two bounds on the same side are comparable, preserve the provenance of the stronger
restriction. The direction depends on the side: a lower bound is stronger when its type is wider,
while an upper bound is stronger when its type is narrower.

```text
Evidence(bool) <= T  and  Validity(int) <= T  =>  Validity(int) <= T
Evidence(int)  <= T  and  Validity(bool) <= T =>  Evidence(int) <= T

T <= Validity(bool)  and  T <= Evidence(int)  =>  T <= Validity(bool)
T <= Evidence(bool)  and  T <= Validity(int)  =>  T <= Evidence(bool)
```

This prevents a weaker evidence bound from relabeling a stronger declaration-imposed restriction
as evidence. When both restrictions are equivalent, prefer evidence: it retains the restriction
while also preserving the inference information. Incomparable same-side bounds do not require a
merged provenance rule: upper-bound intersections and lower-bound unions are represented as
separate constraints, preserving each bound's original `Validity` or `Evidence` tag independently.

Preserve provenance through bound normalization, type-variable reorientation, intersection,
sequent derivation, type mapping, owned-set compaction/loading, and path extraction. Validity and
evidence constraints with identical underlying bounds remain distinct interned constraints. Never
add a single-implication sequent across validity/evidence provenance, even when the underlying
bounds have different strengths: declaration validity must not manufacture inference evidence,
and evidence must not be silently relabeled as validity. This applies both to direct implications
and to reverse implications derived from intersections. Pair-implication sequents may combine
premises with different provenance. Same-subject lattice combinations retain one operand's
provenance only when the resulting type equals that operand; otherwise they become `Mixed`.
Only direct declaration-domain bounds are `Validity`: transitive derivations are `Evidence` only
when every indispensable premise is evidence, and are otherwise `Mixed`. This preserves the one
pure validity equality contributed by a constrained TypeVar's declared alternative on every
complete path. Continue using the underlying types to detect genuinely incompatible effective
restrictions.

Propagate provenance through accumulated `PathBound` values and factored `UpperBound` clauses.
`default_solve` and custom `choose` callbacks select concrete solutions, so they must be able to
inspect validity versus evidence directly. Do not erase provenance merely to preserve the existing
`PathBound` field types.

Provide narrow helpers or constructors so callers can extract underlying types, inspect
provenance, and construct ordinary evidence bounds. Process both bounds directly: logical-default
validity bounds behave as mathematical identities rather than as a hidden optional-bound variant.
Update existing constructors and callers to use provenance-aware bounds directly, and update
actual `PathBound` consumers as needed. The internal `ty_extensions` testing API should offer
`ConstraintSet.lower_bound(lower, typevar)` and `ConstraintSet.upper_bound(typevar, upper)` for
one-sided evidence; `ConstraintSet.range(lower, typevar, upper)` continues to record explicit
evidence on both sides, including `Never` and `object`.

Do not add witness flags to `InteriorNodeData`, propagate booleans through `PathAssignments`, or
introduce separate witness-specific fuel accounting. Bound-side provenance supersedes that earlier
design.

When collecting paths:

- Preserve validity and evidence in the `PathBound` representation rather than exposing only an
    unannotated effective type.
- Accumulate lower bounds into two unions: one containing all evidence lower bounds and one
    containing all validity lower bounds. Their union is the effective lower restriction.
- Preserve provenance individually for factored upper-bound clauses.
- Include both validity and evidence in effective solution restrictions.
- Derive variance and gradual/static argument classification only from evidence bounds.
- Do not infer bindings for variables that have no positive evidence.
- Keep valid empty solutions and genuine alternatives intact.

The two lower unions preserve the original evidence even when a stronger validity lower bound
controls the effective restriction. Preserve witnessed bare-TypeVar relationships after validity
constraints are accumulated, but do not commit in advance to dedicated relationship metadata;
first determine what the new provenance-aware lower/upper bounds already make possible.

### Preserve existing gradual behavior while restricting concrete sequents

`Constraint::new_node_with_bounds` already accepts gradual bounds despite a stale claim that only
fully static bounds are supported. In the static-sequent phase, concrete endpoint containment,
overlap, contradiction, and pivot proofs must require structurally static endpoints and use
constraint-set subtyping rather than assignability. Solver TypeVars are opaque symbolic atoms for
this eligibility check. Where recursive cycle handling is required, reuse the existing Salsa-tracked
owned assignability relation: structural eligibility makes it equivalent to subtyping for concrete
endpoints while still allowing symbolic TypeVars to carry an existing gradual witness. Preserve the
single-range constructor invariant that its derived relation is satisfiable rather than treating a
settled-unsatisfiable range as an expected candidate to skip.

Structurally universal TypeVar rules may still carry an existing gradual witness through a
relationship. Keep the existing dedicated same-TypeVar, different-TypeVar, and nested-TypeVar
constructor paths as that boundary; do not introduce general witness provenance machinery. Keep
single-range derivation assignability-based initially, with the existing PR #26873 TODO, and stop
if focused evidence shows that this path itself creates invalid closure.

Keep validity domains inactive during the static-sequent phase. Phase 5 develops domain-conjoined
path extraction and solution selection with the new boundary active, and must preserve distinct
gradual/static alternatives and existing gradual evidence without a provenance-dependent relation,
equality exception, TDD abstraction special case, or ad hoc solution-union rule.

Existing generic-function behavior must nevertheless be preserved:

```py
def identity[T: (Any, int)](value: T) -> T: ...

reveal_type(identity("x"))  # Any
reveal_type(identity(1))  # int
```

The analogous `list[Any]`/`list[int]` behavior must also remain unchanged. Keep general gradual
assignability unchanged; restrict sequent changes to the static concrete-proof boundary and the
validity/evidence propagation rules described above. Do not compensate by expanding declared
gradual equalities into bottom/top materialization ranges.

### Prefer narrowly subsumed complete paths

Add an explicit shared pruning operation on `PathBounds`, before solution extraction discards the
validity bounds and inference evidence needed to compare declared alternatives. Read variance and
gradual/static argument classification directly from each `PathBound`; do not add variance or
other path metadata to `TypeVarSolution`.

Compare complete paths conservatively:

1. Both paths must contain the same type-variable identities.
1. All type-variable bounds except one must be identical.
1. The differing bounds must belong to a declared constrained type variable and identify exact
    declared alternatives through their validity bounds. Every such complete path must expose one
    exact declared validity equality; assert this invariant rather than accepting or skipping a
    path whose validity bounds were polluted by a derived constraint.
1. Both paths must retain the same inference evidence and compatible evidence-derived variance.
1. Lower-side evidence prefers the narrower compatible declared alternative.
1. Upper-side-only evidence prefers the broader compatible declared alternative.
1. Mutually assignable alternatives prefer static over gradual; otherwise retain declaration
    order.
1. Preserve both paths if multiple type-variable bounds differ, their evidence differs, the
    alternatives are incomparable, or the variable is not declared constrained.
1. Preserve ambiguous gradual argument evidence rather than arbitrarily choosing one compatible
    declared alternative.

For example, lower-side evidence prefers the first of:

```text
{T = str,         U = bytes}
{T = str | bytes, U = bytes}
```

But neither of these dominates the other:

```text
{T = str,         U = bytes}
{T = str | bytes, U = int}
```

Pruning is opt-in for concrete-specialization consumers: callers explicitly prune `PathBounds`
before extracting solutions. Do not prune automatically inside `PathBounds::compute` or
`PathBounds::solve_with`; internal `ConstraintSet.solutions()` and `solutions_for()` APIs should
continue returning their raw solution paths unless a specific implementation expectation conflicts
with required user-visible behavior.

### Recover specific diagnostics only after solving fails

Domain-aware solving can eliminate invalid declarations before `default_solve` or its caller's
callback runs. Preserve existing `MismatchedBound` and `MismatchedConstraint` diagnostics without
expanding `Solutions`:

1. Solve the domain-conjoined set normally.
1. Only if it is unsatisfiable, inspect paths from the original unconjoined constraint set.
1. Reuse `SpecializationBuilder::specialization_error_from_failed_bounds` to distinguish declared
    mismatches from unrelated unsatisfiability.
1. Keep the extra traversal off the successful-solving hot path.
1. Preserve concise diagnostic output and existing user-visible diagnostic coverage.

## Phase status and dependencies

- [x] Phase 1: retain bound type-variable instances in constraint-set arenas.
- [x] Phase 2: replace optional constraint bounds with validity/evidence bounds.
- [x] Phase 3: reintroduce support-derived validity-domain construction.
- [x] Phase 4: preserve evidence-derived variance and add complete-path pruning.
- [x] Static-sequent integration phase: restrict concrete sequents and characterize temporary
    regressions in a non-mergeable intermediate revision. Change `kqntvqkmuuqp`, commit
    `148c3d7cf8cbcce6bdab30fb5b77bd6926b8c42d`.
- [x] Phase 5 activation checkpoint: activate the domain-conjoined TDD, reserve pure `Validity`
    bounds for direct declaration domains, classify every transitive derivation with a validity
    premise as `Mixed`, assert that constrained-TypeVar paths expose one exact validity equality,
    and preserve accepted compatibility debt behind xfail comments. Change
    `qtllpywmtvzrnswnvkqptllkykwpqqzp`, commit
    `2f3d8b49b29b532ca8c629efa9eb785cab1ea20e`.
- [ ] Phase 5 structural completion: replace legacy declaration-aware default solving with solving
    from the effective domain-conjoined path bounds. Accepted behavioral regressions may remain as
    passing `XXX` expectations.
- [ ] Potential Phase 6: restore relationship preservation and the remaining recorded compatibility
    and performance regressions, then select a landable tip.

Phases depend on all preceding phases and must execute in order. Every phase has its own `[π]`
revision, relevant documentation/tests, and a passing full test suite. The earlier abandoned Phase 5
prototype and standalone fully-static revisions are diagnostic evidence only. Complete Phase 5 from
its recorded activation checkpoint; do not revive `pvynqtrq` or copy its rejected workarounds.

The static checkpoint passes all 816 `ty_python_semantic` tests, all 8,908 workspace tests, workspace
clippy, and prek. It records seven `XXX: Phase 5` markers covering gradual range algebra,
`Container[T]`, recursive protocol inference, and mutable `TypedDict` correlation. A constraint-order
wobble audit found that every non-normal mode already failed on the pre-static Phase 4 baseline. The
static phase adds one temporary mask-4 mismatch for the mutable `TypedDict` case: normal ordering
now reports the characterized `object` regression while mask 4 already retains the final `int`
behavior. A potential Phase 6 must restore the correlation and make those outcomes converge; do not
update a wobble expectation to preserve the intermediate diagnostic.

### Phase 1: retain bound type-variable instances in constraint-set arenas

Suggested revision description: `[π] Retain bound type-variable instances in constraint arenas`.

1. Store `BoundTypeVarInstance` rather than only `BoundTypeVarIdentity` in builder-local and owned
    type-variable arenas.
1. Continue using bound identities for `typevar_cache`, support IDs, stable ordering, and
    occurrence equality.
1. Preserve the first instance for each identity; do not duplicate entries when eager/lazy
    evaluation produces another instance for the same occurrence.
1. Update compacted owned overlays, identity-cache initialization, `typevar_data`, loading, and
    affected assertions without threading unnecessary APIs through unrelated interners.
1. Add only the focused coverage needed for retained-instance lookup, identity deduplication, and
    owned-set round trips.
1. Run the full suite and verify that inference behavior is unchanged.

### Phase 2: replace optional constraint bounds with validity/evidence bounds

Suggested revision description: `[π] Distinguish constraint validity from inference evidence`.

1. Introduce the `Validity(Type)`/`Evidence(Type)` bound enum and replace the two optional sides
    of `ConstraintBounds`. Represent missing lower and upper bounds as `Validity(Never)` and
    `Validity(object)`, respectively.
1. Convert every existing relationship constraint to evidence bounds so current behavior remains
    unchanged.
1. Preserve explicit `Evidence(Never)` and `Evidence(object)`, logical-default materialization,
    support collection, source ordering, and type traversal.
1. Update normalization, bound projection, intersection, sequent derivation, type-variable
    reorientation, owned-set compaction/loading, and type mapping to preserve or combine per-side
    provenance correctly. For comparable bounds on the same side, inherit the provenance of the
    stronger restriction: the wider lower bound or narrower upper bound. Preserve the existing
    implication relation for underlying types.
1. Intern otherwise-identical validity and evidence bounds separately. Never add a
    single-implication sequent across different provenance, including reverse implications derived
    from intersections. Allow pair-implication sequents with mixed-provenance premises, using the
    operation-aware same-subject rule for intersections and joining every indispensable premise's
    provenance for transitive bounds.
1. Keep `ConstraintBound` helpers small and local; avoid mechanical renames or broad API rewrites
    beyond the required representation change.
1. Add narrow tests only for important bound/provenance invariants not already protected by
    user-visible tests.
1. Run the full suite and confirm that all-evidence constraints preserve existing inference.

### Phase 3: reintroduce support-derived validity-domain construction

Suggested revision description: `[π] Derive type-variable validity domains from TDD support`.

1. Reintroduce only the narrow `valid_specializations`-style helper needed to create declared
    validity restrictions.
1. Encode declared upper bounds as validity upper bounds and declared constrained alternatives as
    exact validity bounds on both sides.
1. Preserve gradual declared bounds and constraints directly. In particular, encode an `Any`
    constraint as exact validity `T = Any`, not `Never <= T <= object`; do not apply deferred
    bottom/top materialization when accumulating effective path bounds. Remove stale documentation
    claiming all constraint bounds must be fully static.
1. Return `ALWAYS_TRUE` for unbounded variables and `P.args`/`P.kwargs`.
1. Build the full domain by walking existing root support in stable builder-local order and
    conjoining every relevant type variable's validity restrictions.
1. Preserve source-order sidecars, owned-builder overlays, existing Boolean short-circuiting, and
    the distinction between absent and explicit logical-default bounds.
1. Keep the new domain inactive in ordinary solution extraction until Phase 5. Do not restore
    deleted satisfaction APIs or deleted implementation-only mdtests.
1. Add focused coverage for exact gradual alternatives and the representation needed by
    user-visible bounded/constrained generic behavior.
1. Run the full suite before marking the phase complete.

### Phase 4: preserve evidence-derived variance and add complete-path pruning

Suggested revision description: `[π] Preserve evidence variance and compare complete paths`.

1. Propagate validity/evidence provenance through `ConstraintBoundsBuilder`, `PathBound`, and
    factored `UpperBound` clauses so both `default_solve` and custom callbacks can inspect it.
1. Accumulate lower bounds into separate evidence and validity unions rather than one
    provenance-erasing union. Preserve provenance individually for factored upper-bound clauses.
1. Preserve effective lower/upper restrictions while deriving variance and gradual/static
    classification only from evidence.
1. Update contextual inference, generic specialization, receiver handling, pattern narrowing,
    diagnostic helpers, and other real `PathBound` consumers to inspect provenance-aware fields
    directly.
1. Preserve witnessed relationships between compatible constrained type variables. Prefer using
    the new provenance-aware path bounds directly; introduce specialized relationship metadata only
    if the representation proves insufficient.
1. Ensure the ordinary visitor and `compute_simple_bound_conjunction` agree, including canonical
    reorientation of constraints involving another type variable.
1. Leave variables without positive evidence unsolved while preserving valid empty solution paths.
1. Keep `TypeVarSolution` limited to the type variable and its selected solution; derive variance
    and gradual/static evidence directly from `PathBound` when comparing paths.
1. Implement explicit whole-path pruning on `PathBounds` without changing solver callbacks or
    automatically pruning raw solver results.
1. Cover narrow versus broad preference, static versus gradual preference, declaration-order ties,
    incomparable or independently evidenced alternatives, and correlated paths where necessary.
1. Delay behavior-changing consumer integration until Phase 5.
1. Run the full suite before marking the phase complete.

### Static-sequent integration phase

Suggested revision description: `[π] Restrict concrete sequents to fully static bounds`.

This revision is an implementation checkpoint, not a landable tip.

1. Add focused coverage showing that `int -> Any -> str` assignability cannot become transitive
    sequent closure. Define structural sequent eligibility locally in `constraints.rs`, treating
    solver TypeVars as opaque and rejecting actual dynamic concrete components.
1. Apply the gate per participating endpoint to concrete implication, overlap/intersection, pair
    impossibility, contradiction, and pivot proofs. Use constraint-set subtyping for ordinary
    eligible concrete proofs. Where recursive cycle handling is required, reuse the existing
    Salsa-tracked owned assignability query; structural eligibility makes it equivalent to
    subtyping for concrete endpoints without starting an inconsistent second coinductive query.
1. Audit symbolic and derived sequent constructors. Preserve structurally universal propagation of
    an existing gradual witness, but prevent gradual concrete pivots or unrelated TDD branches from
    manufacturing evidence. Preserve the single-range constructor invariant that its derived
    relation is satisfiable rather than silently accepting or skipping an invalid range.
1. Keep support-derived validity domains and behavior-changing pruning consumers inactive. Do not
    compensate in TDD abstraction or solution combination.
1. Run focused generic-function, constraint-algebra, and ordering tests. For each changed behavior,
    assert the observed intermediate result and add an adjacent `XXX` that states the final behavior
    the activation checkpoint or potential Phase 6 must restore or classify. Do not disable tests, weaken
    unrelated assertions, or accept panics as temporary behavior.
1. Run `ty_python_semantic`, the full suite, clippy, snapshot review, and prek. Record the project
    change and commit IDs as a non-mergeable intermediate checkpoint.

### Phase 5: activate domains and complete structural path solving

**Approved checkpoint scope clarification:** Domain activation may persist while some correlated
TypeVar relationships and other inference behavior remain behind adjacent `XXX` expectations. That
permission defers behavioral restoration only; it does not defer replacement of the legacy
`PathBounds::default_solve` declaration logic. Phase 5 is complete only when solving uses the
effective domain-conjoined path bounds without re-reading declarations or rebuilding alternatives.

The activation checkpoint:

- Projects non-inferable variables before deriving and conjoining validity domains, then extracts
    paths using the conjoined support without a second projection.
- Enables complete-path pruning in specialization consumers while asserting that every complete
    constrained-TypeVar domain path retains one exact validity equality. Invalid producer state is
    never accepted or skipped.
- Preserves declaration-specific diagnostics by inspecting unconjoined paths only after a
    domain-aware solve is unsatisfiable.
- Preserves all indispensable premise provenance: transitive derivations remain `Evidence` only
    when every premise is evidence, and any validity premise makes the result `Mixed`. Pure
    `Validity` is reserved for direct declaration-domain constraints.
- Retains the legacy declaration lookup in `PathBounds::default_solve`; removing that lookup and
    solving structurally from path bounds is the remaining Phase 5 work.
- Accepts, for now, concrete alternative unions such as `int | str` and `Left | Right` where a bare
    evidence relationship such as `T := S` should survive. Relationship restoration and
    ambiguous-`Any` behavior are potential Phase 6 work, not reasons to retain the legacy solver.
- Records all observed mdtest changes with adjacent TODO/XXX xfail comments stating the intended
    behavior, including gradual inference, inherited generic constructors, cycles, recursive
    protocols, quantification, and mutable mapping/TypedDict behavior.

Validation at the checkpoint passed all 820 `ty_python_semantic` tests, all 8,913 workspace tests,
workspace clippy, snapshot review,
and `/home/dcreager/bin/jpk`. A local Pydantic ecosystem comparison found six additional
diagnostics consistent with the recorded inherited-generic and relationship losses. It also
measured a repeatable median type-check time of about 2.91 seconds versus 0.384 seconds at the
pre-activation parent; audit and resolve that regression before landing.

**Stop-and-escalate gate for the remaining Phase 5 work:** Run focused tests after each small
implementation step. If any test fails unexpectedly, stop implementation immediately and switch to
diagnosis-only work. Determine the root cause, using
temporary debug instrumentation when useful, and report the exact command, failure, current diff,
and supporting evidence. Propose potential fixes—ideally multiple alternatives—with their
tradeoffs, but do not retain production-code or mdtest behavior changes, implement a fix, update an
expectation, or continue to a later Phase 5 step without explicit user approval. The same gate
applies if a characterized Phase 5 regression does not recover in the expected way. Preserve
intended mdtest assertions as xfails, including `# error: [static-assert-error]`, unless the user
explicitly approves a final expectation change.

Phase 5 completion checklist:

- [x] Inventory the static-phase `XXX` expectations and update every observed activation result with
    an adjacent follow-up expectation.
- [x] Conjoin the support-derived validity domain before path extraction in both direct and
    Salsa-cached entry points while preserving source ordering and the trivial-domain fast path.
- [x] Produce separate valid paths for declared constrained alternatives and reject incompatible
    specializations during traversal without reintroducing gradual sequent closure.
- [x] Preserve one exact pure validity equality for every constrained-TypeVar domain path, classify
    derived constraints as `Mixed`, and assert the invariant during complete-path pruning.
- [ ] Replace `PathBounds::default_solve` with solving from effective structural path bounds without
    re-reading `TypeVarBoundOrConstraints` or rebuilding declared alternatives. Remove the
    superseded declaration-specific selection code and helpers that become unused.
- [ ] Preserve evidence-derived variance, effective lower/upper restrictions, absent versus explicit
    bounds, and unsolved variables structurally. Compatible TypeVar relationships, constrained
    `Any`/`int`, `list[Any]`/`list[int]`, ambiguous gradual evidence, and unbounded `Container[T]`
    may remain as explicitly characterized Phase 6 regressions.
- [x] Explicitly prune subsumed complete paths in affected specialization consumers before
    extracting solutions and combining bindings independently; leave raw internal solution APIs
    exhaustive.
- [x] Recover declaration-specific diagnostics from unconjoined paths only after a domain-aware
    solve fails. Do not apply domains before eager quantification or expand this work into the
    separate quantifier-replacement workstream.
- [ ] Keep accepted temporary constraint-algebra and user-visible changes covered by adjacent
    `XXX` expectations; final classification and restoration may occur in Phase 6.
- [ ] Leave the cache-growth, path-fuel, determinism, and
    `ty_micro[pydantic_core_schema_dict]` performance audit for Phase 6 unless structural solving
    introduces a new regression beyond the activation checkpoint.
- [ ] Remove superseded TODOs and obsolete declaration-handling code after replacement coverage
    passes. Run focused tests, `ty_python_semantic`, the full suite, clippy, snapshot review, and
    prek before marking Phase 5 complete.

## Focused regression coverage

Prefer extending existing user-visible behavior sections:

- `crates/ty_python_semantic/resources/mdtest/generics/legacy/functions.md`
    - Exact constrained-TypeVar promotion.
    - Static versus gradual declared alternatives, including arguments that match only the gradual
        alternative.
    - Narrowest lower-bound and broadest upper-bound-only preferences.
    - Ambiguous gradual evidence.
    - Bounded callable inference and concise mismatch diagnostics.
    - Forwarding compatible constrained type variables while preserving their relationships.
- `crates/ty_python_semantic/resources/mdtest/generics/pep695/functions.md` for corresponding
    modern generic syntax.
- `crates/ty_python_semantic/resources/mdtest/regression/2799_constraint_correlation.md` for
    correlated generic-protocol solutions.
- `crates/ty_python_semantic/resources/mdtest/regression/constraint_set_ordering.md` for source
    ordering, genuine alternatives, absent positive evidence, and stable results; update
    implementation-specific expectations when necessary.
- `crates/ty_python_semantic/resources/mdtest/type_properties/constraints.md` for narrowly
    relevant constraint invariants and missing versus explicit bounds, not as an excuse to lock in
    incorrect implementation details.
- `crates/ty_python_semantic/resources/mdtest/type_properties/quantification.md` only to confirm
    this change does not interfere with the separate eager-quantification workstream.
- Existing sections of `crates/ty_python_semantic/resources/mdtest/call/function.md`,
    `crates/ty_python_semantic/resources/mdtest/call/overloads.md`, and diagnostic files when
    corresponding user-visible behavior changes.

Run a focused mdtest, substituting its resource-relative path:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_DEV_LTO=off \
    INSTA_FORCE_PASS=1 INSTA_UPDATE=always \
    CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
    cargo nextest run -p ty_python_semantic --test mdtest \
    -- mdtest::generics/legacy/functions.md
```

Run the crate suite while iterating:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_DEV_LTO=off \
    INSTA_FORCE_PASS=1 INSTA_UPDATE=always \
    CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
    cargo nextest run -p ty_python_semantic
```

Run the full suite at the end of each phase:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 CARGO_PROFILE_DEV_LTO=off \
    INSTA_FORCE_PASS=1 INSTA_UPDATE=always \
    CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
    cargo nextest run
```

Use the documented `cargo test` fallback if needed. Never run multiple Cargo commands in parallel
in the same workspace.

After snapshot-generating tests:

```sh
find crates -name '*.pending-snap' -print
jj diff --stat
jj diff --git
```

Run prek from the Jujutsu workspace:

```sh
/home/dcreager/bin/jpk
```

## Risks and remaining open decisions

- **Bound-side provenance merging:** comparable same-side bounds inherit the provenance of the
    stronger restriction: the wider lower bound or narrower upper bound. Incomparable lower bounds
    and upper bounds remain separate constraints. Path aggregation retains separate evidence and
    validity lower unions and individually tagged upper clauses, preserving provenance for
    `default_solve` and custom callbacks.
- **Provenance-sensitive sequents:** evidence and validity constraints are separate interned
    propositions. Never add single-implication sequents across provenance, regardless of the
    underlying bounds' relative strength, including reverse implications from simplified
    intersections. Pair-implication sequents can combine mixed-provenance premises: same-side
    intersections inherit the stronger restriction's provenance, while transitive derivations
    produce evidence only when all contributing premises are evidence.
- **Logical identity versus provenance:** provenance-aware bounds can cause otherwise identical
    ranges to intern separately. Logical implication, sequent equivalence, source order, and TDD
    size must remain correct; avoid assuming provenance tags change the actual set of legal
    specializations.
- **Derived constraints:** bound provenance must survive transitive, nested, canonicalized, and
    intersected sequents without converting declaration-only restrictions into spurious evidence
    or polluting the direct declaration-domain validity equality. Transitive derivations produce
    evidence only when every indispensable premise is evidence; any validity or mixed premise
    produces `Mixed`. Pure `Validity` is reserved for direct declaration-domain constraints.
- **Compatible TypeVars:** conjoining `T`'s declared finite domain with witnessed `T = S`
    currently turns `T = S` into concrete `T = int`/`T = str` alternatives. A potential Phase 6
    must preserve the symbolic relationship for compatible constrained variables, including
    compatible subsets and callbacks with redundant bounds, while rejecting incompatible or merely
    overlapping domains. A dedicated `PathBound` field is not prescribed.
- **Gradual alternatives and sequents:** store declared gradual alternatives directly as exact
    validity constraints, without bottom/top materialization. The static-sequent phase prevents
    non-transitive gradual assignability from collapsing paths; a potential Phase 6 must restore
    existing `Any`/`int` and `list[Any]`/`list[int]` behavior. Do not reinterpret provenance as a
    typing relation or add equality-specific sequent behavior.
- **Arena overlays:** retain full type-variable instances while continuing to intern by identity
    and avoiding unstable Salsa-ID-based ordering or unnecessary `db` parameter churn.
- **Fast-path performance:** preserve `compute_simple_bound_conjunction` and its no-sequent-cache
    guarantees. Watch the existing `ty_micro[pydantic_core_schema_dict]` scenario.
- **Empty evidence paths:** negated alternatives and unrelated disjuncts can legitimately produce
    empty complete solutions; validity bounds alone must not manufacture inferred bindings.
- **Variance direction:** lower-only evidence maps to `Contravariant`; upper-only evidence maps to
    `Covariant`. Preserve the existing narrower/broader solution preferences.
- **Complete-path correlations:** only compare paths differing in one constrained variable while
    retaining identical inference evidence; do not create independently combined assignments or
    prune raw solver output.
- **Diagnostics:** invalid paths may disappear before callbacks run; reconstruct specific
    declaration errors only on the diagnostic failure path.
- **Ordering:** merge source-order sidecars consistently and retain declaration-order tie breaks
    without depending on Salsa IDs or arbitrary TDD variable ordering.
- **Quantification:** domains are applied at solution extraction only. Do not expand the scope to
    eager quantifiers being replaced elsewhere.
