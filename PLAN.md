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
- Do not change Python's general gradual-type assignability or declaration semantics. Phase 5 now
    owns the narrower sequent correction needed by domain activation: concrete logical proofs must
    use fully static bounds and subtyping, while structurally valid symbolic rules may propagate an
    existing gradual witness. Do not choose a relation based on validity/evidence provenance.

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

Use `jj` for source control and inspect changes with `jj diff --git` or `jj diff --stat`. Run prek
in this Jujutsu workspace through `/home/dcreager/bin/jpk`. Never modify snapshot files or inline
snapshot bodies manually: regenerate them with the documented test commands, inspect every
snapshot change, and check for `.pending-snap` files when inline snapshots are involved.

## Joint Phase 5: domain activation and fully static sequents

Phase 5 and fully static sequent handling are one semantic integration unit. Investigation showed
that neither has a useful independently passing boundary:

- The sequent map treats gradual assignability as transitive logical implication. Exact validity
    alternatives such as `T = Any` and `T = int` can therefore become mutually implicative and be
    collected into one aggregate path.
- A prototype static-sequent gate on the Phase 5 diagnostic stack preserved the expected `int`
    result for the typed-dictionary union ordering regression and passed the full constraint-set
    ordering mdtest after cycle-safe derived-candidate validation.
- The same prototype exposed a Phase 5 pruning assumption: gradual constrained-TypeVar paths do
    not always present one exact equality validity bound. The current pruner panics instead of
    conservatively retaining such paths.
- Removing gradual closure also exposes independent alternative solutions. In particular,
    unbounded `Container[T]` inference still selects `object` rather than preserving `Any`; domain
    pruning cannot repair an unbounded variable.

Do not revive either rejected workaround from the standalone prototype: selectively retaining TDD
uncertain branches during abstraction, or locally unioning gradual solutions in `generics.rs`.
Likewise, do not add a provenance-dependent typing relation, an equality-only exception, broad
witness metadata, or a general correlation-preserving solution representation without another
approved replan.

The incomplete Phase 5 revision `pvynqtrq` and its pause revision are diagnostic snapshots, not
part of the implementation path. Restart the joint phase from `sqkowvys` (the completed Phase 4
stack plus the raw-graph fix), which is the parent of this replanning revision. The detailed static
sequent requirements remain in
`~/.pi/memory/plans/ruff/fully-static-sequents/PLAN.md`; this document is authoritative for the
combined phase order and completion marker.

The intended landing order is:

1. completed domain-solving Phases 1–4 and the raw-graph fix;
1. the combined Phase 5 domain activation and fully static sequent revision;
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
premises with different provenance. For same-side intersections, inherit the provenance of the
stronger restriction. For transitive derivations, emit evidence only when every contributing
premise is evidence; any contributing validity premise makes the derived bound validity. Continue
using the underlying types to detect genuinely incompatible effective restrictions.

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
fully static bounds are supported. In the joint phase, concrete endpoint containment, overlap,
contradiction, and pivot proofs must require structurally static endpoints and use constraint-set
subtyping rather than assignability. Solver TypeVars are opaque symbolic atoms for this eligibility
check. Use an owned Salsa-tracked relation where cycle recovery is required, and validate and skip
derived candidates that settle as unsatisfiable after a coinductive cycle.

Structurally universal TypeVar rules may still carry an existing gradual witness through a
relationship. Keep the existing dedicated same-TypeVar, different-TypeVar, and nested-TypeVar
constructor paths as that boundary; do not introduce general witness provenance machinery. Keep
single-range derivation assignability-based initially, with the existing PR #26873 TODO, and stop
if focused evidence shows that this path itself creates invalid closure.

Domain-conjoined path extraction and solution selection must be developed with this sequent
boundary active. Preserve distinct gradual/static alternatives and existing gradual evidence
without a provenance-dependent relation, equality exception, TDD abstraction special case, or
ad hoc solution-union rule.

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
    declared alternatives through their validity bounds. If either bound does not expose one exact
    declared validity alternative, preserve both complete paths rather than panicking.
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
- [ ] Phase 5: jointly restrict concrete sequents, solve the domain-conjoined TDD, and simplify
    default solving.

Phases depend on all preceding phases and must execute in order. Every phase has its own `[π]`
revision, relevant documentation/tests, and a passing full test suite. The abandoned Phase 5 and
standalone fully-static revisions are diagnostic evidence only. Implement the joint phase in a new
revision based on this replanning revision; do not layer it onto `pvynqtrq` or copy its rejected
workarounds.

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
    from intersections. Allow pair-implication sequents with mixed-provenance premises, preserving
    the stronger bound's provenance for same-side intersections and deriving validity for
    transitive bounds whenever any contributing premise is validity.
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

### Phase 5: jointly activate domains and restrict concrete sequents

Suggested revision description: `[π] Solve domains with fully static sequents`.

1. Add focused coverage showing that `int -> Any -> str` assignability cannot become transitive
    sequent closure. Define structural sequent eligibility locally in `constraints.rs`, treating
    solver TypeVars as opaque and rejecting actual dynamic concrete components.
1. Apply the gate per participating endpoint to concrete implication, overlap/intersection, pair
    impossibility, contradiction, and pivot proofs. Use constraint-set subtyping for eligible
    concrete proofs and an owned Salsa-tracked query for recursive cycle handling.
1. Audit symbolic and derived sequent constructors. Preserve structurally universal propagation of
    an existing gradual witness, but prevent gradual concrete pivots or unrelated TDD branches from
    manufacturing evidence. Validate provisionally discovered coinductive candidates and skip
    those that settle as unsatisfiable.
1. Conjoin the support-derived validity domain with the original root before path extraction in
    both direct and Salsa-cached entry points. Preserve source ordering and the simple concrete-bound
    fast path whenever possible.
1. Produce separate valid paths for declared constrained alternatives and reject incompatible
    specializations during traversal. Diagnose any aggregated gradual validity path before changing
    abstraction or solution combination; do not reintroduce unsound closure merely to restore the
    old shape.
1. Make complete-path pruning conservative: if a differing constrained-TypeVar bound does not
    expose an exact declared equality validity bound, retain both paths rather than panicking.
    Normal domain alternatives should still expose exact validity bounds so existing specificity,
    static-over-gradual, and declaration-order preferences continue to work.
1. Simplify `PathBounds::default_solve` so it selects from effective path bounds without inspecting
    `TypeVarBoundOrConstraints` or rebuilding declared alternatives independently.
1. Preserve witnessed variance, gradual evidence, bounded lower/upper behavior, absent versus
    explicit bounds, unsolved variables, and relationships between compatible type variables.
    Preserve current constrained `Any`/`int`, `list[Any]`/`list[int]`, ambiguous gradual evidence,
    and unbounded `Container[T]` behavior without a local solution-union heuristic.
1. Explicitly prune subsumed complete paths in affected specialization consumers before extracting
    solutions and combining bindings independently; leave raw internal solution APIs exhaustive.
1. Recover declaration-specific diagnostics by inspecting unconjoined paths only after a
    domain-aware solve fails. Do not apply domains before eager quantification or expand this phase
    into the separate quantifier-replacement workstream.
1. Audit cache growth, path fuel, deterministic ordering, and the
    `ty_micro[pydantic_core_schema_dict]`-sensitive fast path. Favor existing user-visible tests;
    change implementation-focused expectations only when semantically required.
1. Remove superseded TODOs and obsolete declaration-handling code after replacement coverage
    passes. Run focused tests, `ty_python_semantic`, the full suite, clippy, snapshot review, and
    prek before marking the joint phase complete.

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
    intersected sequents without converting declaration-only restrictions into spurious evidence.
    Start conservatively: transitive derivations with any validity premise produce validity, while
    all-evidence derivations produce evidence.
- **Compatible TypeVars:** conjoining `T`'s declared finite domain with witnessed `T = S`
    currently turns `T = S` into concrete `T = int`/`T = str` alternatives. Preserve the symbolic
    relationship for compatible constrained variables, including compatible subsets and callbacks
    with redundant bounds, while rejecting incompatible or merely overlapping domains. The
    user-visible behavior is required; a dedicated `PathBound` field is not prescribed.
- **Gradual alternatives and sequents:** store declared gradual alternatives directly as exact
    validity constraints, without bottom/top materialization. The joint phase must prevent
    non-transitive gradual assignability from collapsing their paths while preserving existing
    `Any`/`int` and `list[Any]`/`list[int]` behavior. Do not reinterpret provenance as a typing
    relation or add equality-specific sequent behavior.
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
