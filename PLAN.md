# Inferability review follow-up plan

## Overview

The current branch replaces preliminary `remove_noninferable` projection with a custom,
support-aware solution walker. The walker traverses the original TDD using
`PathAssignments::walk_edge`, relies on existing sequent propagation, returns bindings only for
inferable variables, and short-circuits subtrees that cannot affect those bindings.

Carl Meyer's review of the original implementation identified several remaining issues. Straightforward
coverage and documentation fixes are complete. Subsequent investigation traced all three remaining
review concerns to existing sequent-map, quantification, solution-selection, or ordering
limitations. They are documented below for handoff, but are outside the scope of this PR.

## Working agreements

- Perform the phases in order unless this document explicitly identifies an independent phase.

- Before resuming work, inspect the implementation and tests to verify that the status markers are
    still accurate; do not assume that a previous agent's status is correct.

- Give each phase its own Jujutsu revision. Never edit an existing revision: create a new revision
    with `jj new -A @` when downstream revisions should incorporate the changes, and describe it with
    a `[π]` prefix.

- Documentation and tests are cross-cutting concerns, not separate phases. Update focused mdtests,
    Rust unit tests, comments, and benchmarks as part of the phase that changes the behavior.

- The full `ty_python_semantic` suite must pass at the end of every phase. If a later phase must
    correct behavior that cannot yet be fixed, assert the current incorrect result and add a clear
    TODO describing the desired result; never comment out or ignore a failing test.

- Run tests without updating snapshots:

    ```sh
    CARGO_PROFILE_DEV_OPT_LEVEL=1 \
    CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    INSTA_UPDATE=no \
    MDTEST_UPDATE_SNAPSHOTS=0 \
    cargo nextest run -p ty_python_semantic --no-fail-fast --status-level fail
    ```

- Run `/home/dcreager/bin/jpk run --files <every changed path>` at the end of each phase.

- When checking perturbed orderings, use `TY_CONSTRAINT_SET_ORDER=normal`, `reverse`, and XOR masks
    `1`, `2`, `3`, `4`, `7`, `8`, and `15`. Some unrelated mdtests already fail under perturbed
    orderings; compare against the pre-phase baseline rather than treating those existing failures as
    new regressions.

- Preserve the current architecture: use the custom walker and `PathAssignments::walk_edge`; do not
    depend on `PathVisitor`, extend `PathFold`, include fuel in memoization keys, introduce
    `fixed_noninferable_bindings`, or add a separate hidden-variable validation mechanism.

- Preserve symbolic references to genuinely unfixed outer variables and preserve meaningful empty
    alternatives alongside informative alternatives.

- Do not use whole-solution deduplication or solution ranking as a workaround. Those are separate
    future concerns; equivalent hidden branches should instead collapse through sound support-aware
    traversal and path normalization.

## Current implementation and relevant files

- `crates/ty_python_semantic/src/types/constraints/solutions.rs`: custom solution walking,
    support-aware path normalization, hidden-only feasibility search, source-order preservation, and
    inferable-only bound extraction.
- `crates/ty_python_semantic/src/types/constraints/support.rs`: support closure over all variables
    mentioned by positive or negative path assignments.
- `crates/ty_python_semantic/src/types/constraints.rs`: constraint construction, full nested support
    collection, sequent generation, `PathAssignments::walk_edge`, and solution selection.
- `crates/ty_python_semantic/resources/mdtest/type_properties/constraints.md`: direct
    `ConstraintSet` coverage for inferable and non-inferable variables.
- `crates/ty_python_semantic/resources/mdtest/regression/noninferable_projection_to_terminal.md`:
    rigid outer variables, compatible and incompatible outer/inner domains, and contextual-return
    regressions.
- `crates/ty_python_semantic/resources/mdtest/bidirectional.md`: contextual constructor, callback,
    and factory regressions blocked on upstream PR #26680.
- `crates/ruff_benchmark/benches/ty.rs`: independent hidden-alternatives Criterion benchmark.

## Scope and explicit non-goals

The following are already documented and are **not** prerequisites for the phases below:

- Repairing incomplete relationship discovery or contradiction propagation in the existing sequent
    map. This PR consumes the sequent map as it exists; fixes belong in separate solver work.
- Resolving issues already marked with TODOs in existing mdtests or source comments.
- Addressing constraint-order dependence holistically; that is being handled in separate work.
- Representing declared bounds and finite domains directly in the TDD. The existing
    `incompatible_finite_noninferable`, `incompatible_bounded_noninferable`, and
    `negative_finite_noninferable` mdtests document why separate hidden-domain validation would be
    the wrong architectural fix.
- The `int` versus `object` TypedDict regression, which depends on representing declared constraints
    in the same TDD domain as other alternatives.
- Contextual-return inference changes blocked on
    [#26680](https://github.com/astral-sh/ruff/pull/26680). The mdtests explicitly distinguish
    diagnostics that should remain, disappear, or change their expected type.
- Global deduplication or ranking of fully solved alternatives.

## Phase 0 — Address straightforward review feedback [COMPLETE]

- Add passing mdtests where a bounded or finitely constrained outer type variable is passed to an
    inner callable whose bound or finite domain is incompatible.
- Add passing coverage for positive and negative constraints whose non-inferable subject contains
    the inferable variable inside `list[I]`, including a negative nested alternative alongside a
    positive `I = int` alternative.
- Clarify the desired fate of every currently expected contextual-return and argument diagnostic.
    In the bounded-union receiver regression, keep the return diagnostic with a more precise found
    type and remove both bogus receiver-argument diagnostics once #26680 lands.
- Keep the TypedDict, declared-domain, and fixed-hidden-variable TODOs documented rather than
    attempting an unrelated architectural fix.

## Phase 1 — Relational contradictions [OUT OF SCOPE: EXISTING SEQUENT LIMITATION]

### Problem

Support calculation already walks nested type arguments, but relationship detection and negative
sequent propagation do not consistently reject impossible assignments.

The following direct-constraint reproductions currently return a solution even though their paths
are inconsistent:

```py
# N = list[int] and I = int imply N = list[I], contradicting the negative decision.
constraints = ConstraintSet.range(list[int], N, list[int])
constraints &= ~ConstraintSet.range(list[I], N, list[I])
constraints &= ConstraintSet.range(int, I, int)
# Current: tuple[Solution[I=int]]
# Expected: no satisfiable solution.

# The problem is not limited to nested bounds.
constraints = ConstraintSet.range(int, N, int)
constraints &= ~ConstraintSet.range(I, N, I)
constraints &= ConstraintSet.range(int, I, int)
# Current: tuple[Solution[I=int | N]]
# Expected: no satisfiable solution.
```

### Diagnosis

These failures are not specific to the custom solution walker. For both reproductions,

```py
static_assert(contradiction == ConstraintSet.never())
```

fails: the existing constraint-set equivalence and satisfiability machinery also does not recognize
the contradiction. Requesting solutions with **both** variables inferable still produces a
solution. The walker is therefore consuming the same incomplete relationship information that the
rest of the solver already consumes.

For the bare case, the sequent map derives separate subtype facts but does not establish the exact
`I = N` fact needed to conflict with `¬(I = N)`. `SequentMap::add_sequents_for_pair` and
`PathAssignments::check_pair_implication` currently propagate positive consequences; they do not
supply the missing equality/negative relationship reasoning.

For the nested case, both constraints have the same subject `N`. Consequently,
`SequentMap::add_sequents_for_pair` routes them through `add_concrete_sequents`, rather than
`add_nested_typevar_sequents`, which is only reached when the subjects differ. Intersecting the
invariant bounds produces a union lower bound and returns `IntersectionResult::CannotSimplify`, so
no `I = int` relationship is derived.

Detailed constraint display can also misleadingly omit the negative nested condition. Display uses
`simplify_for_display`, whose existing historical TODO explicitly acknowledges duplicated and
imperfect relationship logic; direct equivalence checks confirm that the underlying constraint set
has not actually become equivalent to the displayed simplified expression.

**Decision:** out of scope. Correcting either case requires extending the existing sequent map or
related constraint simplification, rather than fixing the walker.

## Phase 2 — Fixed hidden relationships [OUT OF SCOPE: EXISTING TODOs]

**Dependency for future work:** relational and invariant sequent improvements.

### Problem

When an exact hidden assignment and a relationship determine an inferable variable, the current
solver retains the hidden variable in the resulting type. A nested relationship with the hidden
variable as its subject can fail to infer the visible variable at all.

```py
constraints = ConstraintSet.range(int, N, int) & ConstraintSet.range(N, I, N)
# Current: Solution[I=int | N]
# Expected: Solution[I=int]

constraints = ConstraintSet.range(int, N, int)
constraints &= ConstraintSet.range(list[N], I, list[N])
# Current: Solution[I=list[N] | list[int]]
# Expected: Solution[I=list[int]]

constraints = ConstraintSet.range(list[int], N, list[int])
constraints &= ConstraintSet.range(list[I], N, list[I])
# Current: no inferred binding for I.
# Expected: Solution[I=int]
```

### Diagnosis

The bare and nested forms have different underlying causes:

1. For `N = int ∧ I = N`, existentially projecting `N` correctly produces `I = int`:

    ```py
    projected = constraints.exists(tuple[N])
    static_assert(projected == ConstraintSet.range(int, I, int))  # Passes.
    ```

    Nevertheless, the existing solution machinery also retains the relational source constraint
    alongside the derived concrete constraint, producing `I = int | N`. The same symbolic
    contamination appears when **both** variables are inferable, so this is not solely a hidden
    projection problem. Existing TODOs already document it in `fixed_noninferable`,
    `fixed_nested_noninferable`, `invariant_declared_upper`, `invariant_explicit_upper`, and the
    `derived_solution` / transitive-chain ordering regressions.

1. For `N = list[int] ∧ N = list[I]`, existentially projecting `N` incorrectly produces `always`,
    rather than `I = int`; even `constraints.satisfies(I = int)` fails. This demonstrates that the
    missing relationship originates before solution extraction, in the existing invariant
    sequent/quantification machinery. `type_properties/quantification.md` already documents related
    invariant inverse-image and witness-sensitive failures under existing TODOs.

`SolutionWalker::finish` only reverses a relationship when a bound is a bare `Type::TypeVar`.
Teaching it to invert arbitrary invariant generic structures would duplicate missing sequent
reasoning and introduce exactly the parallel fixed-binding machinery this PR avoids.

**Decision:** out of scope. The bare failures directly hit existing TODOs; the nested failures
require fixing invariant relationship derivation and existential quantification elsewhere.

## Phase 3 — Overlapping hidden alternatives [OUT OF SCOPE: EXISTING ORDERING WORK]

**Dependency for future work:** fixed-hidden relationship precision and holistic ordering fixes.

### Problem

The current mdtest and benchmark deliberately use hidden alternatives `{str, bytes}` with
`I = int`, ensuring that no shared concrete value can establish a derived relationship. Carl's
original example instead uses `{int, str}`. Under `TY_CONSTRAINT_SET_ORDER=2`, four such hidden
variables currently produce:

```text
Solution[I=int | N0 | N2]
Solution[I=int | N0 | N2]
Solution[I=int | N2]
```

The correct result is one `Solution[I=int]`, regardless of source order, internal constraint order,
or type-variable orientation.

### Diagnosis

A shared concrete type causes the existing sequent map to derive a relationship even when the
original constraints are otherwise independent:

```py
constraints = ConstraintSet.range(int, I, int) & ConstraintSet.range(int, N, int)
reveal_type(constraints.solutions(inferable=tuple[I]))
# Current: tuple[Solution[I=int | N]]
```

`SolutionWalker::visit_node` closes visible support over every derived relationship. Once a sequent
relates `I` and `N`, the hidden variable genuinely enters that closure; the walker cannot discard it
without solving the existing fixed-hidden-variable TODO from Phase 2.

With four `{int, str}` alternatives and `TY_CONSTRAINT_SET_ORDER=2`, this produces three
contaminated or duplicate visible solutions. Asking for **all** variables to be inferable and then
filtering with `solutions_for(I, inferable=tuple[I, N0, N1, N2, N3])` independently produces 16
paths even under normal ordering. Conversely, the existing existential-projection API correctly
reduces those alternatives to `I = int`, confirming that the issue is relationship-sensitive path
selection rather than a new failure to represent the underlying alternatives.

Measured with the debug `ty` executable:

| Hidden alternatives | Normal ordering      | XOR mask 2              |
| ------------------- | -------------------- | ----------------------- |
| 4                   | 0.07 s; one solution | 0.05 s; three solutions |
| 8                   | 0.56 s; one solution | 0.51 s; three solutions |
| 12                  | 0.55 s; one solution | 3.35 s; four solutions  |
| 16                  | 0.59 s; one solution | 4.49 s; one solution    |
| 20                  | 0.66 s; one solution | 4.65 s; one solution    |

This is order-sensitive and can be slower, but the current implementation did not enumerate
1,048,576 returned paths at 20 alternatives. The existing disjoint `{str, bytes}` benchmark
appropriately isolates the walker optimization without conflating it with already known relational
and ordering defects.

Existing TODOs in `regression/constraint_set_ordering.md` already cover symbolic contamination,
type-variable orientation, transitive-chain ordering, and high-fanout sequent behavior.

**Decision:** out of scope. Fixing this example requires the existing fixed-hidden and
constraint-ordering work; adding whole-solution deduplication or special-casing shared concrete
values in this PR would mask the underlying defects.
