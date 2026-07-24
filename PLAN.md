# Inferability-aware constraint-set solution extraction

## Status

- Initial source investigation and plan draft: complete.
- Design decisions: complete; inferability semantics, solver APIs, fixed
    non-inferable binding rules, minimal path handling, and the simplest-first
    performance strategy are agreed.
- Phase 1 — characterize current behavior and agreed semantics: complete.
- Phase 2 — inferability-aware path representation and solving: complete.
- Phases 3–4: not started.
- Existing constraint-set quantification work is out of scope unless this plan is
    explicitly revised.

## Goal

Remove the preliminary `NodeId::remove_noninferable` projection from
`PathBounds::compute`. Instead, extract solutions directly from the original
constraint-set TDD, verify that each path's non-inferable type variables can
satisfy their constraints, and return bindings only for the requested inferable
type variables.

The desired end state is not simply "walk every path and drop some output
bindings": feasibility of non-inferable variables, dependencies between type
variables, projected path semantics, callback behavior, ordering, and existing
performance fast paths must remain correct.

This work does **not** remove or replace `ConstraintSet::reduce_inferable`,
`NodeId::exists`, `ConstraintSet::for_all`, or explicit quantification performed
by generic-callable relation checking. Those operations have separate semantics
and are the subject of an independent effort.

## Design principle

Choose the simplest implementation that satisfies an agreed requirement and is
supported by a concrete test. Reuse existing path traversal, sequent reasoning,
solution selection, type substitution, TDD reductions, and fast paths before
introducing new abstractions. Do not add general DNF minimization, complete
negative-domain solving, dependency-graph solvers, speculative optimizations,
or other generalized machinery unless a focused regression or measured
performance issue demonstrates that it is necessary. If meeting a real
requirement appears to demand a substantially broader design, stop and ask for
guidance instead of expanding scope silently.

## Working agreements

These instructions apply when resuming this plan, even if they are also present
in the repository's `AGENTS.md` files:

1. Read this entire plan, the applicable `AGENTS.md` files, and relevant source
    files before making changes.
1. Validate every existing status marker against the actual repository state;
    do not assume another agent's completion markers are accurate.
1. Perform the implementation phases in order unless this document explicitly
    records an independence relationship.
1. Every implementation phase must be a self-contained unit of work in its own
    Jujutsu revision. Never edit an existing revision. Create the next revision
    with `jj new -A @` when downstream revisions should incorporate the work, and
    describe it with `jj describe -m '[π] ...'`.
1. Documentation and tests are cross-cutting requirements for every phase, not
    separate deferred cleanup phases.
1. The full test suite must pass at the end of each implementation phase. If a
    later phase will correct behavior that is intentionally still wrong, update
    assertions or mdtest expectations to the current result and add a clear TODO
    describing the correct behavior. Do not comment out or ignore failing tests.
1. Never manually edit snapshot files or inline snapshot bodies. Regenerate
    affected snapshots with the documented snapshot-update environment variables,
    inspect every resulting change, and check for `.pending-snap` files when
    inline snapshots are involved.
1. For this Jujutsu workspace, run prek through `/home/dcreager/bin/jpk`,
    explicitly passing every changed path.
1. Update this document whenever a decision is resolved, a phase changes, or a
    phase is completed.

## Relevant implementation

### Existing solution pipeline

`crates/ty_python_semantic/src/types/constraints.rs`:

- `ConstraintSet::solutions` and `ConstraintSet::solutions_with` accept the
    requested `TypeVarSet` and delegate to `NodeId::solutions_with`.

- `NodeId::solutions_with` calls `PathBounds::compute`, followed by
    `PathBounds::solve_with`.

- `Type::assignable_solutions_with_inferable` also invokes
    `PathBounds::compute`, but returns a Salsa-cached `&PathBounds` that is later
    solved directly by several other callers.

- `PathBounds::compute` first attempts
    `compute_simple_bound_conjunction`, then currently executes:

    ```rust
    let node = node.remove_noninferable(db, builder, inferable);
    ```

    before handling terminal nodes, walking paths, deriving sequent-map facts,
    sorting constraints by source order, and accumulating lower/upper bounds.

- `compute_simple_bound_conjunction` currently accepts only conjunctions of
    concrete bounds on inferable type variables and avoids constructing
    `PathAssignments` or sequent maps.

- Constrained `PathBounds` retain the authoritative inferable `TypeVarSet` and
    builder-independent `ConstraintPath` values. Each path contains its
    accumulated bounds, explicit fixed non-inferable bindings, and compact
    inferability flags; terminal variants retain no domain.

- `PathBounds::solve_with(db, builder, choose)` validates surviving
    non-inferable typevars before invoking the callback, invokes the callback
    only for inferable bounds, substitutes fixed non-inferable bindings into
    inferable solutions, and never emits non-inferable top-level bindings.

- `PathBounds::default_solve` checks declared upper bounds and finite
    constraints, but only for type variables whose path bounds actually reach the
    solver.

- Path extraction partitions positive and negative assignments in one pass,
    using positive assignments for bound evidence and both polarities to detect
    inferable conditions. Comprehensive reasoning about negative constraints on
    non-inferable typevars remains intentionally out of scope.

- Bare typevar-to-typevar bounds are represented by constraining whichever
    typevar comes first in the builder-local ordering. `PathBounds::compute`
    creates reciprocal upper/lower bounds for both participating type variables.
    Consequently, an explicit `N = int` can become an aggregated lower bound
    such as `int | I` when the same path also contains `I = N`; the final
    `PathBound` alone may no longer reveal that the original path fixed `N`.

- `BoundTypeVarInstance::is_inferable` checks `BoundTypeVarIdentity`, including
    binding context, freshness, and `ParamSpec` component. Membership must not be
    reduced to source-level typevar identity.

- `NodeId::with_uncertain` performs local TDD reductions, and the ordered TDD
    operators already collapse ordinary formulas such as
    `A or (A and B)`; extracting their paths does not require a general
    implication-aware DNF minimizer.

- `SatisfiedClauses::simplify` and `simplify_one_round` implement separate,
    display-only DNF simplification. Do not reuse, generalize, or recreate that
    machinery for solution extraction.

### Existing projection

`NodeId::remove_noninferable` and `InteriorNode::remove_noninferable` live in
`crates/ty_python_semantic/src/types/constraints.rs`.

The projection uses `abstract_inner` to remove constraints whose subject is
non-inferable. However, it intentionally retains mixed constraints whose lower
or upper bound is a bare inferable typevar so that `I <= N` has the same
meaning regardless of whether the TDD encodes it as a constraint on `I` or on
`N`.

When removing a node, `abstract_inner`:

- still records its assignment in `PathAssignments`;
- derives sequents from that assignment;
- preserves derived facts that concern variables remaining in the diagram;
- unions the true, uncertain, and false branches; and
- rebuilds retained decisions with TDD-aware `ite` semantics.

Consequently, removing the projection is not equivalent to only suppressing
non-inferable output bindings: the projection also collapses non-inferable-only
branches, preserves relevant implications, and can change the resulting TDD's
terminal classification.

The same `abstract_inner` machinery is also used by existential quantification;
any cleanup must preserve that shared behavior. Once the last
`remove_noninferable` caller is removed, delete only code that is genuinely
exclusive to that operation.

### Important consumers

`crates/ty_python_semantic/src/types/generics.rs`:

- `SpecializationBuilder::solve_pending_with` solves a call-wide pending
    constraint set with a caller-supplied selection hook.
- `SpecializationBuilder::add_type_mappings_from_constraint_set` solves
    individual relations and captures typevar-declaration errors from failed
    bounds.
- Both paths currently post-process cross-typevar artifacts and merge solutions
    into specialization mappings.
- A nearby performance note documents an earlier attempt to skip
    `remove_noninferable`: it changed `LiteralString` precision and did not avoid
    expensive path traversal.

Direct `PathBounds::solve` / `solve_with` consumers include:

- `crates/ty_python_semantic/src/types/call/bind.rs`, including contextual
    return-type inference and variance accumulation;
- `crates/ty_python_semantic/src/types/infer/builder.rs`, including collection,
    constructor, and generator contextual inference; and
- `crates/ty_python_semantic/src/types/narrow.rs`, including exact invariant
    solutions for class-pattern narrowing.

Some selection hooks mutate variance maps or impose exact-bound requirements.
They must not be invoked for non-inferable type variables merely because those
variables remain present in the unprojected TDD.

### Existing tests and benchmark

- `crates/ty_python_semantic/resources/mdtest/type_properties/constraints.md`
    now characterizes inferable-only and non-inferable-only paths, mixed
    relationships, exact and one-sided non-inferable bounds, finite-domain
    declarations and declared upper bounds, negative inferable decisions, and
    correlated
    inferable outputs.
- `crates/ty_python_semantic/resources/mdtest/regression/constraint_set_ordering.md`
    contains source-order, bare-typevar orientation, transitivity,
    non-inferable-output, negation, uncertain-branch, derived-fuel, and
    generic-callable regressions.
    Its `noninferable_nested` section verifies that non-inferable `U` does not
    appear in a returned solution.
- `crates/ty_python_semantic/resources/mdtest/regression/noninferable_projection_to_terminal.md`
    protects a real-world call-inference case where eliminating non-inferable
    constraints currently produces the `always` terminal.
- `crates/ty_python_semantic/resources/mdtest/regression/2799_constraint_correlation.md`
    covers correlated generic-protocol overload solutions.
- `crates/ty_python_semantic/resources/mdtest/type_properties/quantification.md`
    covers fixed non-inferable typevars, nested invariant relationships,
    correlated inferable outputs, finite domains, and negative polarity. Some
    failures there belong to explicit quantification and must not silently
    expand this task's scope.
- `crates/ty_python_semantic/src/types/constraints.rs` contains unit tests that
    verify simple conjunctions do not populate sequent caches.
- `crates/ruff_benchmark/benches/ty.rs` defines
    `ty_micro[pydantic_core_schema_dict]`, a minimized large-union contextual
    inference regression specifically associated with projection/path traversal.
- `TY_CONSTRAINT_SET_ORDER` perturbs both TDD-variable ordering and the local
    typevar ordering used to orient mixed constraints. The
    `.agents/skills/wobbling-ty-constraint-order/SKILL.md` workflow runs mdtests
    under normal, reversed, and several XOR-masked orderings. Wobble runs must
    never update snapshots.

## Phase 1 characterization and baseline

Seventeen focused solver unit tests and expanded mdtest coverage confirm the
following current behavior:

- An inferable concrete constraint returns its inferable binding; a
    satisfiable non-inferable-only path and a non-inferable-only disjunct both
    become `Unconstrained`.
- Independent non-inferable constraints preserve inferable bindings and their
    source order. Bare mixed relationships retain the symbolic outer typevar
    in either orientation, but incorrectly emit the non-inferable typevar as
    another binding and invoke caller selection hooks for that typevar.
- `N = int and I = N` currently returns
    `{I = N | int, N = I | int}`. An explicitly fixed nested relationship
    currently returns `{I = list[int] | list[N], N = int}`. Both cases must
    instead return only the inferable binding with `N` substituted by `int`.
- A one-sided lower or upper bound retains a symbolic reference to `N`; it must
    not be mistaken for evidence that `N = int`.
- A positive constraint incompatible with a non-inferable typevar's declared
    upper bound or finite domain is currently projected away. An inferable
    callback may therefore run even though the original path has no compatible
    non-inferable value.
- Complete negative finite-domain reasoning remains deliberately deferred.
    Negative inferable decisions still produce a constrained empty
    binding path rather than `Unconstrained`.
- Fresh occurrences use their full `BoundTypeVarIdentity`, not the source-level
    identity. Non-inferable-dependent alternatives retain the original inferable
    pairings. Nested calls preserve rigid, bounded, constrained, and nested
    outer-scope typevars, and the existing projected-terminal regression remains
    unchanged.

A sequencing constraint exposed by these characterization tests: while Phase 2
still calls `remove_noninferable`, positive non-inferable constraints that the
projection completely removes are no longer available to `PathBounds`. Phase 2
can filter hooks and validate only non-inferable bounds that survive that
projection; complete declaration validation, rejected-path callback isolation,
and recovery of all explicitly fixed bindings necessarily become observable in
Phase 3 after traversal switches to the original TDD. Do not introduce a second
pre-projection path traversal merely to force those checks into Phase 2.

Baseline before the characterization changes:

- Normal-order mdtests: **479 passed**.

- Focused existing solver/mdtest checks: **8 passed**.

- Optimized debug-profile pydantic microbenchmark:
    **20.338 ms** point estimate; confidence interval
    **[20.027 ms, 20.761 ms]**, using 20 samples, 1 second of warmup, and
    2 seconds of measurement. Reuse the same profile and Criterion settings
    when comparing later phases.

- Existing perturbed-order failures, unchanged after the new tests:

    | Ordering | Passed | Failed | Existing failing mdtest files                                                                          |
    | -------- | -----: | -----: | ------------------------------------------------------------------------------------------------------ |
    | normal   |    479 |      0 | —                                                                                                      |
    | reverse  |    473 |      6 | ordering, recursive protocol, constraint properties, quantification, implication properties, TypedDict |
    | 1        |    474 |      5 | ordering, recursive protocol, constraint properties, quantification, TypedDict                         |
    | 2        |    476 |      3 | ordering, recursive protocol, quantification                                                           |
    | 3        |    473 |      6 | ordering, recursive protocol, constraint properties, quantification, implication properties, TypedDict |
    | 4        |    477 |      2 | ordering, quantification                                                                               |
    | 7        |    473 |      6 | ordering, recursive protocol, constraint properties, quantification, implication properties, TypedDict |
    | 8        |    478 |      1 | ordering                                                                                               |
    | 15       |    473 |      6 | ordering, recursive protocol, constraint properties, quantification, implication properties, TypedDict |

    Here “ordering” means
    `regression/constraint_set_ordering.md`; “recursive protocol” means
    `regression/3954_recursive_protocol_structural_relation.md`; “constraint
    properties” means `type_properties/constraints.md`; “quantification” means
    `type_properties/quantification.md`; “implication properties” means
    `type_properties/implies_subtype_of.md`; and “TypedDict” means
    `typed_dict.md`. None of these pre-existing failures should be silently
    corrected or absorbed into this feature. Baseline and final Phase 1 wobble
    logs are under `$HOME/.pi/tmp/witness-aware-phase1-baseline` and
    `$HOME/.pi/tmp/witness-aware-phase1-final-wobble`, respectively.

- After adding the characterization coverage, the `ty_python_semantic` crate
    passes **785 tests**, with **35 skipped**, and the full workspace passes
    **8,574 tests**, with **45 skipped**. The normal-order mdtest count remains
    **479**, because additional cases were added to existing files. The
    snapshot-updating full-workspace command also regenerated two unrelated
    existing Windows-line-ending parser snapshots; their diffs were reviewed
    and restored rather than included in this revision.

## Phase 2 implementation and validation

Constrained `PathBounds` now retain their inferable `TypeVarSet` and an ordered
boxed slice of `ConstraintPath` values. A path stores its accumulated bounds,
fixed non-inferable bindings, whether a positive or negative assignment is
inferable, and whether its bounds include a non-inferable typevar. The
representation contains no builder-local `ConstraintId`, so
`Type::assignable_solutions_with_inferable` remains safely Salsa-cached.

`PathBounds::solve_with(db, builder, choose)` now validates all surviving
positive non-inferable bounds with the existing `default_solve` logic before
invoking any caller callback. The callback sees only inferable variables, and
returned bindings contain only those variables. Concrete non-inferable typevar
equalities that survive projection are collected before reciprocal bound
aggregation and substituted through bare and nested inferable solution types.
One-sided bounds retain symbolic outer variables. Negative inferable conditions
remain distinct from a genuinely unconstrained non-inferable-only path.

The direct `solve_with` users in call binding, collection inference, and class
pattern narrowing now provide their database and builder. The concrete
conjunction fast path still skips sequent construction and marks its paths as
having no non-inferable bounds, avoiding additional inferability checks while
solving. A small targeted fast-path improvement was justified by an initially
noisy
benchmark regression; no duplicate traversal or alternate TDD representation
was introduced.

Phase 2 also resolves the non-inferable top-level output leak previously
assigned to Phase 3 because callback isolation and output filtering share the
same inferable-domain check. The existing ordering and direct-solution mdtests
now assert the corrected output. Two additional unit tests verify that
non-inferable upper-bound and finite-domain violations invalidate their path
before inferable selection callbacks run. A non-inferable constraint that the
still-active `remove_noninferable` projection erases entirely remains
unavailable to the solver and is explicitly deferred to Phase 3.

Validation after Phase 2:

- `ty_python_semantic`: **787 passed**, **35 skipped**.
- Full workspace with documented snapshot-update settings: **8,576 passed**,
    **45 skipped**. As in Phase 1, the same two unrelated Windows-line-ending
    parser snapshots were regenerated, reviewed, and restored.
- Normal-order mdtests: **479 passed**. Every reversed and XOR-masked run
    retained exactly the Phase 1 failing-file set and pass/fail counts; logs
    are under `$HOME/.pi/tmp/witness-aware-phase2-final-wobble`.
- Pydantic microbenchmark: **21.729 ms**, confidence interval
    **[21.460 ms, 22.106 ms]**, using the same optimized development profile
    and Criterion settings. Host contention made isolated runs misleading;
    paired, alternating executables averaged **22.055 ms** for the Phase 1
    parent and **21.480 ms** for Phase 2 under the same conditions, showing no
    material regression. Benchmark logs are under
    `$HOME/.pi/tmp/witness-aware-phase2`.

## Required semantic cases

Characterize and validate each of the following. Some cases document current
bugs and require an agreed decision before their expected behavior is changed.

1. **Inferable-only conjunction**

    ```text
    inferable = {I}
    I = int
    => {I = int}
    ```

    Preserve the concrete-conjunction fast path and existing cache behavior.

1. **Non-inferable-only satisfiable path**

    ```text
    inferable = {I}
    N = int
    => Unconstrained
    ```

    Do not emit `{N = int}` or a constrained path with an empty inferable binding
    list when the correct public result is `Unconstrained`.

1. **Mixed inferable and non-inferable typevar constraints**

    ```text
    inferable = {I}
    N = int and I = str
    => {I = str}
    ```

    Keep the inferable solution and omit the non-inferable binding.

1. **Rigid outer-scope relationship**

    ```text
    inferable = {I}
    I = N
    => {I = N}
    ```

    Filtering applies to emitted bindings, not blindly to references appearing
    inside inferable solution types.

1. **Fixed non-inferable typevar dependency**

    ```text
    inferable = {I}
    N = int and I = N       => {I = int}
    N = int and I = list[N] => {I = list[int]}
    ```

    Substitute a non-inferable typevar only when the original path establishes
    its concrete value. Preserve the symbolic relationship for `I = N` when
    `N` is not otherwise fixed; do not choose an arbitrary value merely
    because `N` has a finite declared domain. Bounds such as `int <= N` or
    `N <= int` do not, by themselves, establish `N = int`.

1. **Impossible declared non-inferable typevar**

    ```text
    inferable = {I}
    N: (int, str)
    N = bytes
    ```

    Current projection can remove this evidence before `default_solve` checks
    `N`'s declaration. This change must reject the path as unsatisfiable after
    checking the non-inferable typevar's positive bounds against its declared
    finite domain. Apply the same rule to declared upper bounds.

1. **Negative finite-domain non-inferable typevar**

    ```text
    inferable = {I}
    N: (int, str)
    N != int and N != str
    ```

    Positive path bounds alone cannot establish whether the non-inferable
    typevar has a valid specialization. Complete reasoning over combinations
    of negative constraints is deliberately deferred; document the limitation
    and preserve existing
    behavior unless negative facts are already handled by ordinary path
    impossibility or sequent reasoning.

1. **Non-inferable-only alternative**

    ```text
    inferable = {I}
    N = int or I = str
    => Unconstrained
    ```

    Do not accidentally infer `I = str` from an optional branch that is
    subsumed by a non-inferable-only branch.

1. **Correlated inferable outputs**

    ```text
    (N = int and I = int and J = list[int])
      or (N = str and I = str and J = list[str])
    ```

    Preserve the original pairings; do not manufacture
    `(I = int, J = list[str])` by merging non-inferable-dependent branches too
    early.

1. **Negated inferable-only constraints**

    ```text
    inferable = {I}
    I != int
    ```

    A path without positive inferable bindings is not automatically
    `Unconstrained` if it still imposes negative inferable conditions.

1. **TDD uncertain branches**

    Verify non-inferable-only and mixed alternatives under the three-way semantics:

    ```text
    [[n ? C : U : D]] = (n and [[C]]) or [[U]] or (not n and [[D]])
    ```

1. **Typevar orientation and freshness**

    Exercise both `I <= N` encodings, reversed declaration order,
    `TY_CONSTRAINT_SET_ORDER=reverse`, XOR masks, and fresh occurrences of the
    same source-level typevar.

1. **Caller-specific hooks**

    Check bounded/constrained generic calls, diagnostic specialization,
    `LiteralString`, contextual collection inference, generator inference, and
    invariant class-pattern narrowing. A non-inferable typevar must not trigger
    an inferable-variable callback or pollute a variance map.

## Agreed decisions

- The operation to eliminate is `remove_noninferable`, not
    `reduce_inferable`.
- The work should use inferability-aware solution selection and output filtering
    rather than reconstructing a TDD with non-inferable constraints removed.
- Explicit existential/universal quantification remains separate and must
    continue working.
- Non-inferable variables must not appear as top-level returned solution
    bindings.
- Legitimate references to outer, non-inferable type variables inside inferable
    solution types must not be discarded indiscriminately.
- **D1, non-inferable validation scope:** adopt the intermediate approach.
    Validate positive non-inferable bounds against declared upper bounds and
    finite constraint domains, and reject paths without compatible
    non-inferable specializations. Do not expand this effort to complete
    reasoning over combinations of negative non-inferable constraints or to
    existing explicit-quantification defects. Negative inferable decisions and
    negative facts already handled by ordinary path/sequent reasoning must
    nevertheless retain their existing semantics.
- **D2, fixed non-inferable binding substitution:** preserve symbolic
    references to non-inferable outer-scope variables unless the current path
    actually fixes one to a concrete value. Substitute path-fixed values
    through bare and nested inferable solution types, preserving the original
    path's correlations. Do not pick an arbitrary member of a declared finite
    domain merely to eliminate a symbolic reference.
- **D3, inferability and solving API:** retain the authoritative inferable
    `TypeVarSet` in the extracted `PathBounds` representation instead of
    repeating it at every solve call or tagging each individual bound. Change
    `PathBounds::solve_with` to accept `db` and a constraint builder, allowing
    it to validate and resolve non-inferable bounds internally while invoking
    the selection callback exclusively for inferable bounds. Preserve the
    existing `solve(db, builder)` convenience API and migrate the three direct
    `solve_with` callers in call binding, collection inference, and class
    pattern narrowing. The Salsa-cached `PathBounds` value must not retain
    builder-local `ConstraintId` values; any durable decision metadata must use
    builder-independent constraint data.
- **D4, projected-path handling:** do not implement a general implication-aware
    antichain, DNF simplification, subsumption engine, or TDD reconstruction.
    Rely on existing ordered-TDD construction and local reductions for ordinary
    Boolean redundancies such as `A or (A and B)`. Limit additional handling
    to recognizing a valid non-inferable-only path as `Unconstrained`,
    distinguishing negative inferable conditions from genuinely
    non-inferable-only paths, and, only if a focused regression requires it,
    removing exactly identical inferable outputs without changing source order
    or path correlations. Prefer compact
    builder-independent path metadata, such as whether a path has any inferable
    positive or negative decisions, over storing a full inferable DNF. A mixed
    constraint is inferable if its subject is inferable **or** a bare lower/upper
    bound is inferable; testing only the subject would make inferability depend on
    typevar orientation and would misclassify `I = N` as non-inferable-only.
- **D5, performance and implementation strategy:** start with the simplest
    correct implementation throughout the project. Preserve the existing
    concrete-conjunction fast path and cache-behavior tests; do not proactively
    extend the fast path to non-inferable-only constraints or add speculative
    optimizations. Measure the pydantic benchmark before and after the
    migration, then introduce the smallest targeted improvement only if a
    concrete correctness or performance regression demonstrates that it is
    necessary.
- **D6, fixed non-inferable binding recognition:** initially recognize fixed
    non-inferable bindings only from explicit positive exact constraints
    already present or derived on the current path. Do not infer a fixed
    binding from a one-sided bound, a convenient `default_solve` choice, a
    finite declaration alone, generalized entailment, or a new dependency
    solver. Reconsider this limitation only if a focused test or real-world
    regression demonstrates that it produces an incorrect result.

## Recognizing fixed non-inferable bindings

The agreed substitution rule requires proof that a path fixes a non-inferable
typevar to one concrete type. However, the current path accumulator merges
direct constraints, sequent-derived constraints, and reciprocal typevar-to-typevar
bounds. For example:

```text
N = int and I = N
```

can produce an aggregate lower bound resembling `int | I` for `N`, even though
one original positive constraint explicitly establishes `N = int`. Conversely,
`PathBounds::default_solve` can select a convenient compatible type for a
constrained non-inferable typevar without proving that its value is uniquely
fixed.

Record explicit positive exact constraints whose lower and upper bounds are the
same concrete type, including exact facts already derived by existing sequent
reasoning, before reciprocal typevar bounds obscure that evidence. Apply those
recorded substitutions to inferable solution types using existing type-mapping
utilities. Do not treat an arbitrary `default_solve` selection, a one-sided
bound, or declared-domain compatibility as proof of a fixed binding.

This deliberately does not infer concrete non-inferable bindings by combining
one-sided bounds, finite declarations, dependency chains, or custom solver
choices. If an actual regression establishes that one of those cases matters,
add the smallest targeted improvement supported by that regression rather than
introducing generalized entailment machinery preemptively.

## Implementation phases

Every phase below requires its own revision, focused tests/docs in that same
revision, and a passing full test suite before continuing.

### [x] Phase 1 — Characterize current behavior and agreed semantics

1. Add focused constraint-solver unit tests for inferable-only paths,
    non-inferable-only paths, mixed paths, both orientations of bare
    relationships, top-level binding filtering, callback isolation, exact
    non-inferable bindings, and one-sided bounds that must remain symbolic.
1. Extend existing mdtest files rather than creating new files where possible:
    use `type_properties/constraints.md` for direct `ConstraintSet.solutions`
    behavior and `regression/constraint_set_ordering.md` for source-order and
    non-inferable-output cases.
1. Add or extend call-inference regressions covering rigid outer-scope typevars,
    bounded or constrained non-inferable typevars, nested dependencies, and
    projected terminal results.
1. Encode current known incorrect behavior with explicit TODO expectations until
    the phase that corrects it; keep the suite green.
1. Record baseline normal-order test results, relevant wobbled-order failures
    that already exist, and the pydantic microbenchmark baseline.
1. Update this plan with all agreed semantics and revise later phases before
    proceeding.

Exit criteria:

- Expected inferable-binding, non-inferable-feasibility, callback, and terminal
    semantics are specified and covered.
- Existing unrelated quantifier failures are clearly identified.
- Baseline correctness and performance are documented.

### [x] Phase 2 — Introduce inferability-aware path representation and solving

Depends on completed Phase 1 and the agreed decisions D1–D6.

1. Store the inferable `TypeVarSet` alongside constrained paths in `PathBounds`
    without changing the existing `remove_noninferable` call yet; terminal
    variants need no domain. Keep the cached representation independent of
    builder-local constraint IDs. Add only compact path metadata needed to
    distinguish non-inferable-only paths from negative inferable conditions.
1. Add `db` and a constraint builder to `PathBounds::solve_with`, preserve
    `solve(db, builder)`, and make the selection hook observable only for
    inferable path bounds.
1. Validate positive non-inferable bounds against declared upper bounds and
    finite domains when those bounds survive the existing preliminary
    projection, without introducing comprehensive negative-domain reasoning.
    Use explicit positive exact non-inferable bindings that remain available
    before reciprocal bound aggregation, preserve cross-typevar relationships,
    and use existing type substitution utilities for those fixed values only.
    Fully projected non-inferable facts remain deferred to Phase 3; do not
    introduce a duplicate pre-projection traversal or alternate TDD
    representation.
1. Validate surviving non-inferable typevars before invoking any
    inferable-variable callback; errors must invalidate only the corresponding
    path, avoid misleading inferable-typevar declaration diagnostics, and
    leave caller-owned callback state unchanged for rejected paths. A
    non-inferable typevar removed entirely by the still-active projection cannot
    be rejected until Phase 3.
1. Preserve fresh bound identity membership, source-order stability, and
    Salsa-cached `PathBounds` compatibility.
1. Migrate `NodeId::solutions_with` and the direct `solve_with` consumers in
    `types/call/bind.rs`, `types/infer/builder.rs`, and `types/narrow.rs`; update
    comments/API documentation in the same revision.
1. Update focused tests and affected snapshots using the documented harness.

Exit criteria:

- Every constrained `PathBounds` carries its authoritative inferable domain;
    terminal variants remain minimal.
- All solution entry points distinguish inferable and non-inferable bounds.
- Non-inferable typevars are never passed to inferable-variable hooks. Facts
    completely erased by the still-active projection remain deferred to Phase 3.
- Existing behavior is preserved while preliminary projection is still present,
    apart from any explicitly agreed inferable-binding filtering introduced here.

### [ ] Phase 3 — Extract directly from the original TDD

Depends on completed Phase 2.

1. Remove `node.remove_noninferable(db, builder, inferable)` from
    `PathBounds::compute`.
1. Walk the original TDD and retain positive non-inferable information plus
    any inferable negative or uncertain decisions required for projected-path
    semantics; do not expand the work to comprehensive negative-domain
    solving. Validate every original positive non-inferable constraint against
    its declaration before any inferable callback, and record explicit positive
    exact bindings before reciprocal bound aggregation. Update the Phase 1 TODO
    expectations for impossible standalone non-inferable typevars and
    rejected-path callback isolation.
1. Preserve sequent-derived inferable facts without reconstructing the original
    projected diagram.
1. Solve non-inferable typevars jointly with inferable path bounds as required;
    retain correlated path families, substitute path-fixed non-inferable values
    through bare or nested types, and preserve unfixed outer-scope references.
1. Classify a valid path with no positive or negative inferable decisions as
    `Unconstrained`; preserve negative inferable conditions and original path
    correlations without introducing general DNF simplification or implication-
    aware subsumption. Deduplicate identical outputs only if a regression
    demonstrates that it is necessary.
1. Preserve Phase 2's non-inferable-output filtering while updating remaining
    TODO expectations for non-inferable facts that the old projection erased.
1. Preserve both mixed-constraint orientations under normal/reversed/XOR-masked
    orderings.
1. Update `noninferable_projection_to_terminal.md` only to reflect the new
    implementation strategy; preserve its actual inference expectation.

Exit criteria:

- No extraction path invokes `remove_noninferable`.
- Every public `Solutions` path contains bindings only for inferable typevars.
- Projected terminal, disjunction, correlation, non-inferable declaration, and
    ordering tests pass with the agreed semantics.

### [ ] Phase 4 — Preserve fast paths, remove dead projection code, and validate

Depends on completed Phase 3.

1. Preserve the existing `compute_simple_bound_conjunction` fast path without
    restoring an up-front TDD projection; extend it only if a measured
    regression demonstrates that the extra complexity is necessary.
1. Verify its unit tests still show that concrete conjunctions do not populate
    the single-constraint or pair-constraint sequent caches.
1. Measure `ty_micro[pydantic_core_schema_dict]` and investigate regressions in
    large-union collection contexts or `LiteralString` precision.
1. Delete `NodeId::remove_noninferable`,
    `InteriorNode::remove_noninferable`, and any comments/caches/helpers that are
    exclusively associated with the removed operation. Preserve `abstract_inner`
    and every explicit existential/universal quantification caller.
1. Run targeted and complete semantic test suites, inspect all snapshot diffs,
    and run mdtests under perturbed constraint order without snapshot updates.
1. Run prek on every changed file and document any remaining intentionally
    deferred semantic limitations.

Exit criteria:

- `rg -n 'remove_noninferable' crates/ty_python_semantic` finds no production
    implementation or obsolete explanatory comments.
- The existing pydantic microbenchmark does not materially regress.
- Normal and perturbed-order mdtests pass except for explicitly documented,
    independently pre-existing failures.
- The full suite and repository hooks pass.

## Validation commands

Normal targeted semantic tests:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
CARGO_PROFILE_DEV_DEBUG=line-tables-only \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic
```

Focused mdtest files:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
CARGO_PROFILE_DEV_DEBUG=line-tables-only \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic -- \
  mdtest::regression/constraint_set_ordering.md

CARGO_PROFILE_DEV_OPT_LEVEL=1 \
CARGO_PROFILE_DEV_DEBUG=line-tables-only \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic -- \
  mdtest::regression/noninferable_projection_to_terminal.md

CARGO_PROFILE_DEV_OPT_LEVEL=1 \
CARGO_PROFILE_DEV_DEBUG=line-tables-only \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic -- \
  mdtest::type_properties/quantification.md
```

Full suite at the end of each implementation phase:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
CARGO_PROFILE_DEV_DEBUG=line-tables-only \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run
```

Use the documented `cargo test` fallback when `cargo nextest` is unavailable.
After any snapshot-updating command, inspect every generated snapshot diff and
check for `.pending-snap` files.

Relevant performance benchmark:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
CARGO_PROFILE_DEV_DEBUG=line-tables-only \
cargo bench --profile dev -p ruff_benchmark --bench ty -- \
  'ty_micro\[pydantic_core_schema_dict\]' \
  --sample-size 20 --warm-up-time 1 --measurement-time 2
```

For ordering perturbation, follow
`.agents/skills/wobbling-ty-constraint-order/SKILL.md`: run mdtests only, unset
`INSTA_FORCE_PASS`, set `INSTA_UPDATE=no` and `MDTEST_UPDATE_SNAPSHOTS=0`, and
run the normal, `reverse`, `1`, `2`, `3`, `4`, `7`, `8`, and `15` orderings
sequentially.

Before completing any changed revision:

```sh
/home/dcreager/bin/jpk run --files PLAN.md <every-other-changed-path>
```
