# Witness-aware constraint-set solution extraction

## Status

- Initial source investigation and plan draft: complete.
- Design decisions: complete; witness semantics, solver APIs, grounding rules,
    minimal path handling, and the simplest-first performance strategy are
    agreed.
- Implementation phases: not started.
- Existing constraint-set quantification work is out of scope unless this plan is
    explicitly revised.

## Goal

Remove the preliminary `NodeId::remove_noninferable` projection from
`PathBounds::compute`. Instead, extract solutions directly from the original
constraint-set TDD, account for non-inferable type variables when establishing
whether a path has a valid witness, and return bindings only for the requested
inferable type variables.

The desired end state is not simply "walk every path and drop some output
bindings": witness feasibility, dependencies between type variables, projected
path semantics, callback behavior, ordering, and existing performance fast paths
must remain correct.

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

- `PathBounds` stores only `Unsatisfiable`, `Unconstrained`, or boxed path
    bounds. The inferable `TypeVarSet` is not retained after `compute` returns.

- `PathBounds::solve_with` presently invokes its callback for every stored
    `PathBound`, invalidates a path when the callback returns `Err(())`, and emits
    a `TypeVarSolution` for each returned `Some(Type)`.

- `PathBounds::default_solve` checks declared upper bounds and finite
    constraints, but only for type variables whose path bounds actually reach the
    solver.

- `PathAssignments::positive_constraints` discards negative and uncertain
    decisions. That is sufficient for the current positive-bound selection model,
    but may be insufficient to prove that a hidden witness can satisfy the whole
    path.

- Bare typevar-to-typevar bounds are represented by constraining whichever
    typevar comes first in the builder-local ordering. `PathBounds::compute`
    creates reciprocal upper/lower bounds for both participating type variables.
    Consequently, an explicit `N = int` can become an aggregated witness lower
    bound such as `int | I` when the same path also contains `I = N`; the final
    `PathBound` alone may no longer reveal that the original path grounded `N`.

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
non-inferable output bindings: the projection also collapses witness-only
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
They must not be invoked for witness-only type variables merely because those
variables remain present in the unprojected TDD.

### Existing tests and benchmark

- `crates/ty_python_semantic/resources/mdtest/regression/constraint_set_ordering.md`
    contains source-order, bare-typevar orientation, transitivity, hidden-output,
    negation, uncertain-branch, derived-fuel, and generic-callable regressions. Its
    `noninferable_nested` section explicitly documents the existing bug where
    non-inferable `U` appears in a returned solution.
- `crates/ty_python_semantic/resources/mdtest/regression/noninferable_projection_to_terminal.md`
    protects a real-world call-inference case where eliminating hidden constraints
    currently produces the `always` terminal.
- `crates/ty_python_semantic/resources/mdtest/regression/2799_constraint_correlation.md`
    covers correlated generic-protocol overload solutions.
- `crates/ty_python_semantic/resources/mdtest/type_properties/quantification.md`
    covers grounded witnesses, nested invariant relationships, correlated visible
    outputs, finite domains, and negative polarity. Some failures there belong to
    explicit quantification and must not silently expand this task's scope.
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

## Required semantic cases

Characterize and validate each of the following. Some cases document current
bugs and require an agreed decision before their expected behavior is changed.

1. **Visible-only conjunction**

    ```text
    inferable = {I}
    I = int
    => {I = int}
    ```

    Preserve the concrete-conjunction fast path and existing cache behavior.

1. **Witness-only satisfiable path**

    ```text
    inferable = {I}
    N = int
    => Unconstrained
    ```

    Do not emit `{N = int}` or a constrained path with an empty visible binding
    list when the correct public result is `Unconstrained`.

1. **Mixed visible and witness constraints**

    ```text
    inferable = {I}
    N = int and I = str
    => {I = str}
    ```

    Keep the visible solution and omit the hidden binding.

1. **Rigid outer-scope relationship**

    ```text
    inferable = {I}
    I = N
    => {I = N}
    ```

    Filtering applies to emitted bindings, not blindly to references appearing
    inside visible solution types.

1. **Grounded witness dependency**

    ```text
    inferable = {I}
    N = int and I = N       => {I = int}
    N = int and I = list[N] => {I = list[int]}
    ```

    Substitute a non-inferable witness only when the original path establishes
    its concrete value. Preserve the symbolic relationship for `I = N` when
    `N` is not otherwise grounded; do not choose an arbitrary value merely
    because `N` has a finite declared domain. Bounds such as `int <= N` or
    `N <= int` do not, by themselves, establish `N = int`.

1. **Impossible declared witness**

    ```text
    inferable = {I}
    N: (int, str)
    N = bytes
    ```

    Current projection can remove this evidence before `default_solve` checks
    `N`'s declaration. This change must reject the path as unsatisfiable after
    checking the witness's positive bounds against its declared finite domain.
    Apply the same rule to declared upper bounds.

1. **Negative finite-domain witness**

    ```text
    inferable = {I}
    N: (int, str)
    N != int and N != str
    ```

    Positive path bounds alone cannot establish witness feasibility. Complete
    hidden-witness reasoning over combinations of negative constraints is
    deliberately deferred; document the limitation and preserve existing
    behavior unless negative facts are already handled by ordinary path
    impossibility or sequent reasoning.

1. **Witness-only alternative**

    ```text
    inferable = {I}
    N = int or I = str
    => Unconstrained
    ```

    Do not accidentally infer `I = str` from an optional branch that is
    subsumed by a witness-only branch.

1. **Correlated visible outputs**

    ```text
    (N = int and I = int and J = list[int])
      or (N = str and I = str and J = list[str])
    ```

    Preserve the original pairings; do not manufacture
    `(I = int, J = list[str])` by merging witness-dependent branches too early.

1. **Negated visible-only constraints**

    ```text
    inferable = {I}
    I != int
    ```

    A path without positive visible bindings is not automatically
    `Unconstrained` if it still imposes negative visible conditions.

1. **TDD uncertain branches**

    Verify witness-only and mixed alternatives under the three-way semantics:

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
    invariant class-pattern narrowing. A hidden witness must not trigger a
    visible-variable callback or pollute a variance map.

## Agreed decisions

- The operation to eliminate is `remove_noninferable`, not
    `reduce_inferable`.
- The work should use witness-aware solution selection and output filtering
    rather than reconstructing a TDD with non-inferable constraints removed.
- Explicit existential/universal quantification remains separate and must
    continue working.
- Non-inferable variables must not appear as top-level returned solution
    bindings.
- Legitimate references to outer, non-inferable type variables inside visible
    solution types must not be discarded indiscriminately.
- **D1, witness-validation scope:** adopt the intermediate approach. Validate
    positive witness bounds against declared upper bounds and finite constraint
    domains, and reject paths without a compatible witness. Do not expand this
    effort to complete reasoning over combinations of negative hidden-witness
    constraints or to existing explicit-quantification defects. Negative visible
    decisions and negative facts already handled by ordinary path/sequent
    reasoning must nevertheless retain their existing semantics.
- **D2, grounded-witness substitution:** preserve symbolic references to
    non-inferable outer-scope variables unless the current path actually
    establishes a concrete value for that witness. Substitute path-grounded
    values through bare and nested visible solution types, preserving the
    original path's correlations. Do not pick an arbitrary member of a declared
    finite witness domain merely to eliminate a symbolic reference.
- **D3, visibility and solving API:** retain the authoritative inferable
    `TypeVarSet` in the extracted `PathBounds` representation instead of
    repeating it at every solve call or tagging each individual bound. Change
    `PathBounds::solve_with` to accept `db` and a constraint builder, allowing
    it to validate and resolve witness-only bounds internally while invoking
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
    to recognizing a valid witness-only path as `Unconstrained`, distinguishing
    negative visible conditions from genuinely witness-only paths, and, only if
    a focused regression requires it, removing exactly identical visible
    outputs without changing source order or path correlations. Prefer compact
    builder-independent path metadata, such as whether a path has any visible
    positive or negative decisions, over storing a full visible DNF. A mixed
    constraint is visible if its subject is inferable **or** a bare lower/upper
    bound is inferable; testing only the subject would make visibility depend on
    typevar orientation and would misclassify `I = N` as witness-only.
- **D5, performance and implementation strategy:** start with the simplest
    correct implementation throughout the project. Preserve the existing
    concrete-conjunction fast path and cache-behavior tests; do not proactively
    extend the fast path to witness-only constraints or add speculative
    optimizations. Measure the pydantic benchmark before and after the
    migration, then introduce the smallest targeted improvement only if a
    concrete correctness or performance regression demonstrates that it is
    necessary.
- **D6, grounded-witness recognition:** initially recognize concrete witnesses
    only from explicit positive exact constraints already present or derived on
    the current path. Do not infer grounding from a one-sided bound, a convenient
    `default_solve` choice, a finite declaration alone, generalized entailment,
    or a new dependency solver. Reconsider this limitation only if a focused
    test or real-world regression demonstrates that it produces an incorrect
    result.

## Grounded-witness recognition

The agreed substitution rule requires proof that a path fixes a hidden witness
to one concrete type. However, the current path accumulator merges direct
constraints, sequent-derived constraints, and reciprocal typevar-to-typevar
bounds. For example:

```text
N = int and I = N
```

can produce an aggregate lower bound resembling `int | I` for `N`, even though
one original positive constraint explicitly establishes `N = int`. Conversely,
`PathBounds::default_solve` can select a convenient compatible type for a
constrained witness without proving that the witness is uniquely fixed.

Record explicit positive exact constraints whose lower and upper bounds are the
same concrete type, including exact facts already derived by existing sequent
reasoning, before reciprocal typevar bounds obscure that evidence. Apply those
recorded substitutions to visible solution types using existing type-mapping
utilities. Do not treat an arbitrary `default_solve` selection, a one-sided
bound, or declared-domain compatibility as grounding.

This deliberately does not infer concrete witnesses by combining separate
one-sided bounds, finite declarations, dependency chains, or custom solver
choices. If an actual regression establishes that one of those cases matters,
add the smallest targeted improvement supported by that regression rather than
introducing generalized entailment machinery preemptively.

## Implementation phases

Every phase below requires its own revision, focused tests/docs in that same
revision, and a passing full test suite before continuing.

### [ ] Phase 1 — Characterize current behavior and agreed semantics

1. Add focused constraint-solver unit tests for visible-only paths,
    witness-only paths, mixed paths, both orientations of bare relationships,
    top-level binding filtering, callback isolation, exact witness grounding,
    and one-sided bounds that must remain symbolic.
1. Extend existing mdtest files rather than creating new files where possible:
    use `type_properties/constraints.md` for direct `ConstraintSet.solutions`
    behavior and `regression/constraint_set_ordering.md` for source-order and
    hidden-output cases.
1. Add or extend call-inference regressions covering rigid outer-scope typevars,
    bounded/constrained witnesses, nested witness dependencies, and projected
    terminal results.
1. Encode current known incorrect behavior with explicit TODO expectations until
    the phase that corrects it; keep the suite green.
1. Record baseline normal-order test results, relevant wobbled-order failures
    that already exist, and the pydantic microbenchmark baseline.
1. Update this plan with all agreed semantics and revise later phases before
    proceeding.

Exit criteria:

- Expected visible-binding, witness-feasibility, callback, and terminal
    semantics are specified and covered.
- Existing unrelated quantifier failures are clearly identified.
- Baseline correctness and performance are documented.

### [ ] Phase 2 — Introduce witness-aware path representation and solving

Depends on completed Phase 1 and the agreed decisions D1–D6.

1. Store the inferable `TypeVarSet` alongside constrained paths in `PathBounds`
    without changing the existing `remove_noninferable` call yet; terminal
    variants need no domain. Keep the cached representation independent of
    builder-local constraint IDs. Add only compact path metadata needed to
    distinguish witness-only paths from negative visible conditions.
1. Add `db` and a constraint builder to `PathBounds::solve_with`, preserve
    `solve(db, builder)`, and make the selection hook observable only for
    inferable path bounds.
1. Validate positive witness bounds against declared upper bounds and finite
    domains, without introducing comprehensive hidden-negative-domain reasoning.
    Record explicit positive exact witness constraints before reciprocal bound
    aggregation, preserve cross-typevar relationships, and use existing type
    substitution utilities for those grounded witness values only.
1. Validate all hidden witnesses before invoking any visible-variable callback;
    errors must invalidate only the corresponding original path, avoid
    misleading visible-typevar declaration diagnostics, and leave caller-owned
    callback state unchanged for rejected paths.
1. Preserve fresh bound identity membership, source-order stability, and
    Salsa-cached `PathBounds` compatibility.
1. Migrate `NodeId::solutions_with` and the direct `solve_with` consumers in
    `types/call/bind.rs`, `types/infer/builder.rs`, and `types/narrow.rs`; update
    comments/API documentation in the same revision.
1. Update focused tests and affected snapshots using the documented harness.

Exit criteria:

- Every constrained `PathBounds` carries its authoritative inferable domain;
    terminal variants remain minimal.
- All solution entry points understand visible versus witness-only bounds.
- Hidden witnesses are never passed to visible-variable hooks.
- Existing behavior is preserved while preliminary projection is still present,
    apart from any explicitly agreed visible-binding filtering introduced here.

### [ ] Phase 3 — Extract directly from the original TDD

Depends on completed Phase 2.

1. Remove `node.remove_noninferable(db, builder, inferable)` from
    `PathBounds::compute`.
1. Walk the original TDD and retain the positive witness information plus any
    visible negative/uncertain decisions required for projected-path semantics;
    do not expand the work to comprehensive hidden-negative-domain solving.
1. Preserve sequent-derived visible facts without reconstructing the original
    projected diagram.
1. Solve hidden witnesses jointly with visible path bounds as required; retain
    correlated path families, substitute path-grounded witness values through
    bare/nested types, and preserve ungrounded outer-scope references.
1. Classify a valid path with no positive or negative visible decisions as
    `Unconstrained`; preserve negative visible conditions and original path
    correlations without introducing general DNF simplification or implication-
    aware subsumption. Deduplicate identical outputs only if a regression
    demonstrates that it is necessary.
1. Correct the existing non-inferable `U` output leak and update its mdtest TODO.
1. Preserve both mixed-constraint orientations under normal/reversed/XOR-masked
    orderings.
1. Update `noninferable_projection_to_terminal.md` only to reflect the new
    implementation strategy; preserve its actual inference expectation.

Exit criteria:

- No extraction path invokes `remove_noninferable`.
- Every public `Solutions` path contains bindings only for inferable typevars.
- Projected terminal, disjunction, correlation, declared-witness, and ordering
    tests pass with the agreed semantics.

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
cargo bench -p ruff_benchmark --bench ty -- \
  'ty_micro\[pydantic_core_schema_dict\]'
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
