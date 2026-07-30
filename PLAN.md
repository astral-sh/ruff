# Inferability review follow-up plan

## Overview

The current branch replaces preliminary `remove_noninferable` projection with a custom,
support-aware solution walker. The walker traverses the original TDD using
`PathAssignments::walk_edge`, relies on existing sequent propagation, returns bindings only for
inferable variables, and short-circuits subtrees that cannot affect those bindings.

Carl Meyer's review of the original implementation identified several remaining issues. Straightforward
coverage and documentation fixes are complete. The phases below cover the issues requiring deeper
investigation, without expanding into already acknowledged future work.

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

## Phase 1 — Propagate relational contradictions soundly [NOT STARTED]

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

### Investigation and implementation

1. Minimize both reproductions in the Rust constraint/sequent unit tests and inspect which
   `PairImplication`, mutual, nested, and negative sequents are generated and applied by
   `PathAssignments::walk_edge`.
2. Determine whether the missing information originates in constraint construction, sequent
   generation, assignment propagation, or hidden-only witness search. Do not assume nested support
   collection is the problem: `intern_mentioned_typevars_in_type` already visits nested arguments.
3. Extend the existing sequent machinery so positive fixed assignments and negative relationship
   decisions produce contradictions consistently for both bare and invariant nested relationships.
4. Add focused positive and negative direct-constraint mdtests. Keep the intentional rule that a
   negative inferable decision alone supplies no positive solution evidence.
5. Verify contradictions are rejected identically across normal, reversed, and XOR-perturbed
   constraint/type-variable orders.

### Completion criteria

- Both reproductions above are unsatisfiable.
- Previously valid negative-only and empty-plus-informative alternatives retain their behavior.
- Full semantic tests pass and perturbed ordering introduces no new baseline failures.

## Phase 2 — Specialize fixed hidden relationships without symbolic leakage [NOT STARTED]

**Dependency:** Phase 1, so contradictory relational assignments are already handled soundly.

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

### Investigation and implementation

1. Trace how `SequentMap` derives exact bare and invariant nested relationships and how
   `SolutionWalker::found_satisfied_path` and `finish` turn those assignments into visible bounds.
2. Determine whether the correct fix belongs in existing relation/sequent derivation, constraint
   canonicalization, visible-bound accumulation, or a combination. Favor the existing sequent
   machinery over a second substitution or fixed-binding subsystem.
3. Ensure genuinely unfixed outer variables remain symbolic. Distinguish exact hidden assignments
   from one-sided bounds, gradual evidence, and unrelated hidden choices without tracking a separate
   map of fixed non-inferable bindings.
4. Update the existing `fixed_noninferable`, `fixed_nested_noninferable`, and invariant-class TODO
   expectations only when the implementation genuinely establishes their correct specialization.
5. Add the hidden-subject `N = list[int] ∧ N = list[I]` regression and its bare and source-order
   variants.

### Completion criteria

- Exact hidden relationships yield concrete visible solutions where logically forced.
- Unfixed outer variables remain symbolic, and inferable-only top-level bindings are preserved.
- Existing gradual, one-sided, rigid-outer, and nested regression tests retain their intended
  results.
- Full semantic tests pass and perturbed ordering introduces no new baseline failures.

## Phase 3 — Make overlapping hidden alternatives order-independent [NOT STARTED]

**Dependency:** Phase 2, because shared concrete types can create legitimate intermediate
relationships between inferable and non-inferable variables.

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

### Investigation and implementation

1. Trace which shared-concrete-value sequents relate `I` to each hidden `N`, and identify why the
   current support closure, relevant-assignment memoization, or witness shortcut retains different
   visible paths for equivalent hidden assignments.
2. After Phase 2 eliminates fixed hidden symbolic leakage, determine whether the remaining branches
   already collapse. If not, strengthen support-aware path normalization or visibility checks while
   preserving source constraints in memoization keys and never adding fuel to those keys.
3. Add direct-constraint mdtests for both disjoint and overlapping alternatives, inferable-first and
   inferable-last source orders, and reversed subject orientation.
4. Extend the existing Criterion benchmark with overlapping `{int, str}` alternatives as a second
   family, keeping the disjoint `{str, bytes}` family. Scale both through 4, 8, 12, 16, and 20
   hidden alternatives and verify the benchmark under normal and perturbed orderings.
5. Compare perturbed-order mdtest failures against the pre-phase baseline. The new overlapping
   coverage itself must remain stable across `normal`, `reverse`, `1`, `2`, `3`, `4`, `7`, `8`, and
   `15`.

### Completion criteria

- Both disjoint and overlapping alternatives produce exactly one `Solution[I=int]`.
- Hidden variables do not leak into visible solution types and equivalent hidden branches do not
  produce duplicate solution paths.
- The benchmark remains practical at 20 alternatives without enumerating 1,048,576 paths.
- Full semantic tests, focused benchmark test mode, ordering checks, and prek pass.
