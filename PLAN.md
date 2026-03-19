# Plan: Propagate nested-typevar constraints through variance in the sequent map

## Status markers

Each step is marked with one of:

- ⬜ Not started
- 🔧 In progress
- ✅ Completed

When resuming this plan, read through the relevant files to validate status
markers before continuing. Include these instructions in the plan even though
they are also listed in the AGENTS.md file.

## Overview

The constraint set's `exists_one` operation removes all constraints mentioning a
bound typevar. When a typevar `T` appears *nested* inside the bound of a
constraint on a different typevar `U` (e.g., `U ≤ Sequence[T]`), we currently
lose all information about `U` because the sequent map doesn't generate derived
facts that substitute `T`'s concrete bounds into the nested occurrence.

The fix requires teaching `add_sequents_for_pair` to recognize when one
constraint provides bounds on a typevar `T`, and the other constraint's
lower/upper bound mentions `T` nested inside a parameterized type. Using the
variance of `T`'s position in that parameterized type, we can determine what
substitution is valid:

- **Covariant**: upper bound on `T` propagates into upper bound position;
    lower bound on `T` propagates into lower bound position. (Preserves
    direction.)
- **Contravariant**: upper bound on `T` propagates into lower bound
    position; lower bound on `T` propagates into upper bound position. (Flips
    direction.)
- **Invariant**: only an equality constraint on `T` (lower = upper) can
    propagate.
- **Bivariant**: the typevar is irrelevant to the type; no implication is
    needed since the constraint doesn't actually depend on the typevar.

We implement covariance first as a complete end-to-end slice, then add
contravariance and invariance as modifications to that working foundation.

## Steps

### Phase 1: Helpers

#### Step 1.1 ✅ — Confirm that `variance_of` serves as our detection mechanism

No new helper is needed. The existing `VarianceInferable` trait provides
`ty.variance_of(db, typevar)`, which returns:

- `Bivariant` if the typevar doesn't appear in the type at all (or is genuinely
    bivariant) — in either case, no implication is needed.
- `Covariant`, `Contravariant`, or `Invariant` if the typevar appears nested
    in the type — telling us which substitution rule applies.

This single call replaces both the presence check (`any_over_type`) and the
variance computation. We just call `variance_of` and skip if the result is
`Bivariant`.

At the call site, add a prominent comment explaining this interpretation:
`Bivariant` here means "the typevar does not appear in this type (or is
genuinely bivariant, which is equivalent for our purposes — no implication is
needed in either case)." Note that if `Bivariant` is ever removed from the
`TypeVarVariance` enum, we would need an alternative representation for
"typevar not present" (e.g., returning `Option<TypeVarVariance>`).

#### Step 1.2 ✅ — Add a `Single` variant to `ApplySpecialization`

Add a new variant to `ApplySpecialization` in `generics.rs`:

```rust
Single(BoundTypeVarInstance<'db>, Type<'db>),
```

This maps a single typevar to a concrete type. The `get` method implementation
is trivial: check if `bound_typevar` matches the stored typevar and return the
mapped type if so, `None` otherwise.

This can then be used via the existing `TypeMapping::ApplySpecialization` to
perform the substitution: `ty.apply_type_mapping(db, &TypeMapping::ApplySpecialization(ApplySpecialization::Single(typevar, replacement)))`.

The rest of the `apply_type_mapping` infrastructure handles the recursive walk
through the type tree automatically.

### Phase 2: Covariant nested typevar propagation (end-to-end)

This phase implements the covariant case fully, including sequent generation,
testing, and validation. Contravariance and invariance will follow as
modifications to this working foundation.

#### Step 2.1 ✅ — Add mdtest cases for covariant propagation (red)

Add tests to the "Transitivity" section of
`crates/ty_python_semantic/resources/mdtest/type_properties/implies_subtype_of.md`.
These follow the existing pattern: construct constraint sets with
`ConstraintSet.range(...)`, combine them with `&`, and verify implications with
`implies_subtype_of`.

Each markdown section has its own scope, so define local `Covariant[T]` (and
later `Contravariant[T]`, `Invariant[T]`) classes in the new test sections.
Test both typevar orderings to verify BDD-ordering independence.

Key test cases for covariant upper bound propagation:

- `(T ≤ int) ∧ (U ≤ Covariant[T])` should imply `U ≤ Covariant[int]`
    (i.e., `constraints.implies_subtype_of(U, Covariant[int])`)
- The same should NOT imply `U ≤ Covariant[bool]` (too tight)

Key test cases for covariant lower bound propagation:

- `(int ≤ T) ∧ (Covariant[T] ≤ U)` should imply `Covariant[int] ≤ U`
    (i.e., `constraints.implies_subtype_of(Covariant[int], U)`)

Run these tests first to verify they fail with the current logic (red phase of
TDD). Then proceed to Step 2.2 to make them pass.

#### Step 2.2 ✅ — Add sequent generation for covariant nested typevars (green)

Currently, `add_sequents_for_pair` dispatches to
`add_mutual_sequents_for_different_typevars` when the two constraints are on
different typevars. That method only handles the case where a typevar appears as
a *direct* `Type::TypeVar(...)` in the other constraint's lower/upper bound.

Add a new method (e.g., `add_nested_typevar_sequents`) called from
`add_mutual_sequents_for_different_typevars` (or from `add_sequents_for_pair`
directly) that handles the case where one constraint's bound *contains* the
other constraint's typevar nested inside a parameterized type.

For the initial covariant-only implementation, the logic is:

Given constraints `(l_B ≤ B ≤ u_B)` and `(l_C ≤ C ≤ u_C)` where `B` appears
nested in `l_C` or `u_C`:

**Case A: `B` appears in `u_C` (upper bound of `C`)**

- Compute `v = u_C.variance_of(db, B)`.
- If covariant: emit `(l_B ≤ B ≤ u_B) ∧ (l_C ≤ C ≤ u_C) → (C ≤ u_C[B := u_B])`
    (The upper bound on `B` substitutes into the upper bound of `C`.)

**Case B: `B` appears in `l_C` (lower bound of `C`)**

- Compute `v = l_C.variance_of(db, B)`.
- If covariant: emit `(l_B ≤ B ≤ u_B) ∧ (l_C ≤ C ≤ u_C) → (l_C[B := l_B] ≤ C)`
    (The lower bound on `B` substitutes into the lower bound of `C`.)

For now, skip the contravariant, invariant, and bivariant cases — just return
early if the variance is not covariant.

Note: the antecedent constraints in these sequents are the *original*
constraints from the BDD. The pair implication is valid because if both original
constraints hold, the covariant substitution certainly follows.

The existing `add_mutual_sequents_for_different_typevars` handles direct
`Type::TypeVar(B)` references in bounds. Since a bare `Type::TypeVar` has
covariant variance in itself, the new nested logic would technically subsume it.
However, the existing code has careful handling for canonical ordering of
typevar-to-typevar constraints, so it's cleaner to keep them separate and only
check for nested references when the bound is *not* a bare `Type::TypeVar`.
Add a comment in the code explaining this relationship — it's a subtle point
that would be easy to miss.

#### Step 2.3 ✅ — Run the full test suite

Run the full constraint set and ty_python_semantic test suite to check for
regressions. Fix any issues before proceeding.

### Phase 3: Contravariant and invariant propagation

Once covariance is working end-to-end, extend the logic to handle the remaining
variance cases. These should be fairly straightforward modifications to the
covariance logic.

#### Step 3.1 ✅ — Add mdtest cases for contravariant and invariant propagation (red)

Add tests following the same pattern as Step 2.2, using the `Contravariant[T]`
and `Invariant[T]` classes already defined in the "Compound types" section.

Contravariant test cases:

- `(T ≤ int) ∧ (U ≤ Contravariant[T])` should imply
    `Contravariant[int] ≤ U` (flipped direction — but actually this doesn't help
    tighten U's upper bound). Need to think carefully about which constraints
    are expressible and testable here.
- `(int ≤ T) ∧ (U ≤ Contravariant[T])` should imply
    `U ≤ Contravariant[int]` (lower bound on T, flipped by contravariance, gives
    upper bound on Contravariant[T]).

Invariant test cases:

- `(T = int) ∧ (U ≤ Invariant[T])` should imply `U ≤ Invariant[int]`.
- `(T ≤ int) ∧ (U ≤ Invariant[T])` should NOT imply `U ≤ Invariant[int]`.

Composed variance test cases:

- Nested generics where variances compose (e.g., `Covariant[Contravariant[T]]`
    — net effect is contravariant).

Run these tests first to verify they fail (red phase).

#### Step 3.2 ✅ — Add contravariant propagation

Extend the nested typevar sequent generation:

**`B` in `u_C` (upper bound of `C`), contravariant:**

- Emit `(l_B ≤ B ≤ u_B) ∧ (l_C ≤ C ≤ u_C) → (C ≤ u_C[B := l_B])`
    (The *lower* bound on `B` substitutes into the upper bound of `C` — flipped.)

**`B` in `l_C` (lower bound of `C`), contravariant:**

- Emit `(l_B ≤ B ≤ u_B) ∧ (l_C ≤ C ≤ u_C) → (l_C[B := u_B] ≤ C)`
    (The *upper* bound on `B` substitutes into the lower bound of `C` — flipped.)

#### Step 3.3 ✅ — Add invariant propagation

Extend the nested typevar sequent generation:

- Only emit a derived constraint if `l_B = u_B` (equality constraint on `B`).
- If so, substitute that single type into either bound of `C`.

#### Step 3.4 ✅ — Run the full test suite

Run the full test suite again to check for regressions.

### Phase 4: Reverse direction — decomposing matching generic bounds

Phases 1–3 implemented the *forward* direction: given a concrete bound on a
typevar `T`, substitute it into a nested occurrence of `T` in another typevar's
bound. This is a cross-typevar (different-typevar) pair sequent.

The *reverse* direction handles the case where a single constraint on a typevar
has lower and upper bounds that are both parameterizations of the same generic
type. By decomposing the generic type using variance, we can extract bounds on
the nested typevar.

Example: the constraint `(Sequence[int] ≤ A ≤ Sequence[T])` implies
`(int ≤ T)`, because `Sequence` is covariant, so `Sequence[int] ≤ Sequence[T]`
implies `int ≤ T`.

This combined constraint arises from two separate constraints `(Sequence[int] ≤ A)`
and `(A ≤ Sequence[T])` being combined — the existing `add_concrete_sequents`
logic should already produce this combined constraint as a pair implication.

This is *part* of what's needed to make patterns like
`invoke(head_sequence, x)` work:

```python
def invoke[A, B](c: Callable[[A], B], a: A) -> B: ...
def head_sequence[T](s: Sequence[T]) -> T: ...
def _(x: Sequence[int]):
    reveal_type(invoke(head_sequence, x))  # should be int
```

The matching would produce constraints that combine into:

- `Sequence[int] ≤ A ≤ Sequence[T]` (from argument + callable parameter)
- `T ≤ B` (from callable return matching)

To derive `int ≤ T`, we need to decompose `Sequence[int] ≤ A ≤ Sequence[T]`
by recognizing that `Sequence[int] ≤ Sequence[T]` must hold, and applying
covariance.

Note: this example will also require conjoining constraint sets across
multiple arguments (which is not yet implemented — tracked by the TODO in
`add_type_mappings_from_constraint_set`). The reverse decomposition is
necessary but not sufficient on its own.

#### Where this fits

This is a single-constraint decomposition: given a constraint
`(L ≤ A ≤ U)`, we check whether `L ≤ U` produces useful derived constraints
on the typevars mentioned in `L` and `U`. This logic belongs in
`add_sequents_for_single` (or is triggered when the combined constraint is
first created).

#### Variance rules (same as forward, applied in reverse)

Given a constraint `(G[l'] ≤ A ≤ G[T])` where both bounds share the same
generic base `G`:

| Variance of T in G | Derived constraint                               |
| ------------------ | ------------------------------------------------ |
| Covariant          | `l' ≤ T`                                         |
| Contravariant      | `T ≤ l'`                                         |
| Invariant          | `T = l'` (only if both sides match structurally) |
| Bivariant          | skip                                             |

And symmetrically for `(G[T] ≤ A ≤ G[u'])`:

| Variance of T in G | Derived constraint                      |
| ------------------ | --------------------------------------- |
| Covariant          | `T ≤ u'`                                |
| Contravariant      | `u' ≤ T`                                |
| Invariant          | `T = u'` (only with matching structure) |
| Bivariant          | skip                                    |

#### Approach: single implication via assignability check

Rather than building bespoke generic-base-matching logic, we can reuse the
existing constraint set assignability machinery. Given a constraint
`(L ≤ A ≤ U)`, the constraint is only satisfiable if `L ≤ U`. If `L` and/or
`U` mention typevars, then `L ≤ U` produces a constraint set on those typevars.

This could be triggered from `add_sequents_for_single` — for every constraint
`(L ≤ A ≤ U)`, compute `L.when_constraint_set_assignable_to(U)` and derive
implications from the result. No need for an upfront check on whether the
bounds mention typevars — just do the CSA check unconditionally and see if the
result is a non-trivial constraint set. Deeply scanning for typevars would not
be significantly cheaper than just doing the check.

This should subsume the existing `single_implications` logic in
`add_sequents_for_single`. The current code manually propagates typevar-to-
typevar bounds (e.g., `(S ≤ T ≤ U) → (S ≤ U)`), but a CSA check on `S ≤ U`
will produce that same constraint with the correct typevar ordering
automatically. The bare-typevar case does not need special treatment — CSA
handles canonical ordering naturally.

Note that the existing pair sequent logic should already combine two
same-typevar constraints like `(Sequence[int] ≤ A)` and `(A ≤ Sequence[T])`
into a single combined constraint `(Sequence[int] ≤ A ≤ Sequence[T])` via
`add_concrete_sequents`. So the decomposition logic lives at the single-
constraint level.

The result of the assignability check is a full constraint set (an arbitrary
boolean formula, not just a conjunction). The sequent map currently only
supports implications where the consequent is a single constraint. As a
pragmatic starting point, we handle only the case where the resulting
constraint set is a single conjunction — i.e., a single path from root to the
`always` terminal in the BDD.

To detect this, we can take advantage of BDD reduction. Our BDDs are only
quasi-reduced, but redundant nodes where both outgoing edges lead to `never`
are still collapsed. This means if we ever encounter an interior node where
both outgoing edges (if_true and if_false) point to something other than
`never`, that node *must* have at least two paths to the `always` terminal,
and the constraint set is not a simple conjunction. So a single structural
walk of the tree suffices to check — no PathAssignments/SequentMap needed.

If the constraint set is simple (single root→always path), walk it a second
time to collect the constraints along that path:

- For each positive constraint (interior node where we take the if_true
    branch): record a `single_implication` from the original constraint to the
    derived constraint.
- For each negative constraint (interior node where we take the if_false
    branch): record a `pair_impossibility` between the original constraint and
    the derived constraint.

For common cases (covariant/contravariant generics with a single type
parameter), the result should always be a simple conjunction, so this approach
should suffice.

A future, more sophisticated solution could handle arbitrary constraint set
results by viewing the BDD as a DNF (disjunction of paths from root to the
`always` terminal, where each path is a conjunction). Each DNF clause could be
integrated into `PathAssignments` by recursing with resets for each path — a
pattern similar to what `walk_edge` already does for true/false/uncertain
branches. But this is significantly more complex and can be deferred.

Circular dependency concern: we'd be invoking constraint set assignability
*from within* sequent map construction. We need to verify this doesn't create
cycles. A possible mitigation is to use the assignability check without
consulting the sequent map (i.e., a "raw" check that doesn't itself rely on
derived facts).

#### Steps

##### Step 4.1 ⬜ — Add mdtest cases for reverse decomposition (red)

Add tests to the Transitivity section of `implies_subtype_of.md`. The tests
should construct the combined constraint directly using `ConstraintSet.range`
(e.g., `ConstraintSet.range(Covariant[int], A, Covariant[T])`) and verify
that the derived implications hold.

Test cases:

- Covariant: `(Covariant[int] ≤ A ≤ Covariant[T])` should imply `int ≤ T`
- Contravariant: `(Contravariant[int] ≤ A ≤ Contravariant[T])` should imply
    `T ≤ int` (flipped)
- Invariant: `(Invariant[int] ≤ A ≤ Invariant[T])` should imply `T = int`
- Bare typevar (existing behavior, should still pass):
    `(S ≤ A ≤ T)` should imply `S ≤ T`
- Test both typevar orderings for BDD-ordering independence

##### Step 4.2 ✅ — Fix the disjointness check in `intersect`

The current `ConstraintId::intersect` method uses
`lower.is_constraint_set_assignable_to(db, upper)` (a universal/always check)
to determine if the intersection `(L ≤ A ≤ U)` is disjoint. When `U` contains
typevars (e.g., `Covariant[T]`), this returns false even if there exist
assignments of `T` that make `L ≤ U` hold — causing the combined constraint to
be pruned as unsatisfiable.

Fix: change the check to an existential one — "is `L ≤ U` *ever* true?" rather
than "is `L ≤ U` *always* true?". This is
`!when_constraint_set_assignable_to(L, U).is_never_satisfied()`. If there
exists some assignment where the intersection is satisfiable, the combined
constraint should survive.

##### Step 4.3 ✅ — Add CSA-based decomposition in `add_sequents_for_single`

For each constraint `(L ≤ A ≤ U)`, compute
`L.when_constraint_set_assignable_to(U)` to get a constraint set `C` that
describes the conditions under which `L ≤ U` holds. Then:

1. Check if `C` is a simple conjunction (single root→always path in the BDD).
    Use the structural criterion: if any interior node has both outgoing edges
    pointing to something other than `never`, the result is not simple — bail
    out.
1. If simple, walk the single path and record sequents:
    - For each positive constraint (if_true branch taken): add a
        `single_implication` from the original constraint to the derived one.
    - For each negative constraint (if_false branch taken): add a
        `pair_impossibility` between the original and derived constraints.

This should subsume the existing `single_implications` logic in
`add_sequents_for_single`, including bare typevar-to-typevar propagation.
Variance is handled automatically by the CSA check — no need for explicit
variance matching or special-casing bare typevars.

##### Step 4.4 ✅ — Verify bare typevar propagation still works

Confirm that the existing tests for typevar-to-typevar transitivity
(e.g., `(S ≤ T ≤ U) → (S ≤ U)`) still pass with the new CSA-based logic.
If the new logic fully subsumes the old `single_implications` code, consider
removing the old code.

##### Step 4.5 ✅ — Run the full test suite

Run the full test suite to check for regressions.

### Phase 5: Handle the same-typevar case with recursive nested references

#### Step 5.1 ✅ — Investigate same-typevar recursive nesting and concrete bound substitution

**Recursive nesting** (`T ≤ Covariant[T]`): Investigated and confirmed that
trivial cases (lower = Never) are handled correctly. Non-trivial cases
(`Covariant[int] ≤ T ≤ Covariant[T]`) are a pre-existing limitation — the
CSA decomposition derives `int ≤ T` but the pair intersection between the
original and derived constraints is flagged as disjoint, which is actually
mathematically correct (the combined constraint is unsatisfiable for most
user-defined classes). Does not arise in practice.

**Concrete bound substitution** (newly identified gap): The forward direction
(Phase 2/3) substitutes B's concrete bounds into C's bounds that contain B.
The reverse direction — substituting B (the typevar) into C's bounds that
contain B's concrete bound — is not yet implemented. This is analogous to the
existing "pivot" cases in `add_mutual_sequents_for_different_typevars` but
generalized to work inside generic types.

Examples (covariant case):

- `(Covariant[BU] ≤ C) ∧ (B ≤ BU) → (Covariant[B] ≤ C)`
- `(C ≤ Covariant[BL]) ∧ (BL ≤ B) → (C ≤ Covariant[B])`

These create cross-typevar links that are weaker than the originals but useful
for downstream inference (e.g., after `exists_one`). Test cases are written
in the mdtest file with TODO markers. Implementation deferred to a future
phase.

### Phase 6: Clean up

#### Step 6.1 ✅ — Remove or update the TODO comment

Remove the TODO comment at line 2828 of `constraints.rs` once the feature is
working. Update any related comments.

#### Step 6.2 ✅ — Run `/home/dcreager/bin/jpk`

Final pre-commit checks. We are in a jj worktree, so use `jpk` (with no
arguments) as a standin for `prek`.

### Phase 7: Typevar bound substitution into nested generic types

Phases 2–3 implemented the *forward* direction of nested typevar propagation:
when B (a typevar) appears nested inside C's bound, substitute B's concrete
bounds for B. This phase implements a restricted *reverse* direction: when one
of B's bounds is a *bare typevar* that appears nested inside C's bound,
substitute B (the typevar) for that bound.

This generalizes the existing top-level "pivot" cases in
`add_mutual_sequents_for_different_typevars` to work inside generic types.
The pivot cases handle:

```text
(CL ≤ C ≤ pivot) ∧ (pivot ≤ B ≤ BU) → (CL ≤ C ≤ B)
```

The new cases handle the same logic when the pivot appears inside a generic:

```text
(CL ≤ C ≤ G[pivot]) ∧ (pivot ≤ B ≤ BU) → (CL ≤ C ≤ G[B])
```

The derived constraint is *weaker* than the original (since `B ≤ BU` means
`G[B] ≤ G[BU]` in a covariant position, so we're relaxing C's bound). But it
introduces a cross-typevar link between B and C that is useful for downstream
inference, especially after `exists_one` removes one of the typevars.

#### Scope restriction: bare typevar bounds only

The fully general version of this feature would handle arbitrary concrete
types nested in C's bounds — e.g., `Covariant[int]` where `int` matches B's
upper bound. However, detecting an arbitrary concrete type inside a nested
position requires different machinery (pattern matching on generic alias
structure, or a CSA-based approach).

When B's bound is a bare typevar, we can reuse `variance_of` for detection,
since `variance_of` already finds typevars nested inside types. This makes
the implementation a natural extension of `add_nested_typevar_sequents`.

For example, given `(G[S] ≤ C) ∧ (S ≤ B)` where S is a typevar:

- `variance_of(G[S], S)` finds S in the covariant position
- We know `S ≤ B`, so `G[S] ≤ G[B]` (covariance)
- Therefore `G[S] ≤ C` implies `G[B] ≤ C` (weakening the lower bound)
- Emit pair implication: `(G[S] ≤ C) ∧ (S ≤ B) → (G[B] ≤ C)`

A TODO comment should be added noting that a future extension could handle
the case where B's bound is an arbitrary type (not just a bare typevar).

#### Where this fits

The existing `add_nested_typevar_sequents` handles the case where `B` (the
constraint's typevar) appears in C's bound — it substitutes B's concrete
bounds for B. This new logic handles the complementary case: a *different*
typevar (one of B's bounds) appears in C's bound — we substitute B for that
typevar, creating a weaker but useful cross-typevar constraint.

Both are called from `add_sequents_for_pair` for different-typevar pairs.
The new logic can live in `add_nested_typevar_sequents` itself, or in a new
method called alongside it.

#### Variance rules

The rules mirror `add_nested_typevar_sequents` but in the *weakening*
direction rather than the *tightening* direction. We substitute B for the
found typevar only when the result weakens (relaxes) the bound.

Given constraints `(l_C ≤ C ≤ u_C)` and `(l_B ≤ B ≤ u_B)`, and a typevar S
that is one of B's bounds:

**S = u_B (B's upper bound is a bare typevar S) and S appears in u_C:**

We know `B ≤ S`. By variance:

- Covariant: `G[B] ≤ G[S]`, so `u_C[B] ≤ u_C[S]` — substituting B *weakens*
    the upper bound → emit `C ≤ u_C[S := B]`
- Contravariant: `G[S] ≤ G[B]`, so `u_C[B] ≥ u_C[S]` — substituting B
    *tightens* → no useful derivation
- Invariant: only if `l_B = u_B` (equality on B), then `B = S` and the
    substitution is exact

**S = l_B (B's lower bound is a bare typevar S) and S appears in l_C:**

We know `S ≤ B`. By variance:

- Covariant: `G[S] ≤ G[B]`, so `l_C[B] ≥ l_C[S]` — substituting B *weakens*
    the lower bound → emit `l_C[S := B] ≤ C`
- Contravariant: `G[B] ≤ G[S]`, so `l_C[B] ≤ l_C[S]` — substituting B
    *tightens* → no useful derivation
- Invariant: only if `l_B = u_B` (equality on B)

**S = l_B and S appears in u_C:**

We know `S ≤ B`. By variance:

- Contravariant: `G[B] ≤ G[S]`, so `u_C[B] ≤ u_C[S]` — substituting B
    *weakens* the upper bound → emit `C ≤ u_C[S := B]`
- Covariant: tightens → no useful derivation
- Invariant: only if `l_B = u_B`

**S = u_B and S appears in l_C:**

We know `B ≤ S`. By variance:

- Contravariant: `G[S] ≤ G[B]`, so `l_C[B] ≥ l_C[S]` — substituting B
    *weakens* the lower bound → emit `l_C[S := B] ≤ C`
- Covariant: tightens → no useful derivation
- Invariant: only if `l_B = u_B`

#### Implementation

Added a new method `try_reverse_nested_typevar_sequents` in the sequent map,
called from `add_nested_typevar_sequents` alongside the existing forward
direction. For each constraint pair (B, C) on different typevars:

1. Extract B's bare typevar bounds (upper/lower bounds that are `Type::TypeVar`).
1. For each such typevar S, use `variance_of` to check if S appears nested
    in C's upper/lower bounds.
1. Based on variance and which bound S is (upper vs lower), determine whether
    substituting B for S *weakens* (relaxes) the bound — only emit in that case.
1. Construct the substituted bound via `ApplySpecialization::Single(S, B)` and
    emit a pair implication.

The variance rules for weakening:

- **Upper bound of C** (want to make it *larger* to weaken):
    - Covariant + S is B's lower bound (S ≤ B → G[S] ≤ G[B]): emit
    - Contravariant + S is B's upper bound (B ≤ S → G[S] ≤ G[B] in contra): emit
- **Lower bound of C** (want to make it *smaller* to weaken):
    - Covariant + S is B's upper bound (B ≤ S → G[B] ≤ G[S]): emit
    - Contravariant + S is B's lower bound (S ≤ B → G[B] ≤ G[S] in contra): emit
- **Invariant**: only when B is an equality constraint (l_B = u_B).

This was needed because the canonical constraint encoding depends on typevar
ordering in the builder: `B ≤ S` may be encoded as `(Never ≤ B ≤ S)` on B
(when S is earlier) or `(B ≤ S ≤ object)` on S (when B is earlier). The
existing forward direction in `add_nested_typevar_sequents` only handles the
second encoding. The new reverse direction handles both.

#### Steps

##### Step 7.1 ✅ — Add tests for bare typevar bound substitution (red)

Added tests covering covariant and contravariant cases, both typevar
orderings. All 8 assertions initially failed.

##### Step 7.2 ✅ — Implement `try_reverse_nested_typevar_sequents` (green)

Added the reverse direction logic. All 8 assertions now pass.

##### Step 7.3 ✅ — Run the full test suite and jpk

Updated the `tdd_owned_round_trip` test to match the new `load` ordering
behavior. Full test suite (513 tests) passes. jpk passes.

## Open questions

1. **Performance**: The `variance_of` computation walks the type tree
    recursively. Calling it inside `add_sequents_for_pair` for every pair of
    constraints could be expensive. We should check if caching or early-exit
    (e.g., skip if neither bound contains any typevar via `any_over_type`) is
    needed.

1. **Multiple typevar occurrences**: A single bound like `dict[T, T]` mentions
    `T` twice. The `variance_of` implementation already handles this by joining
    the variances of all occurrences. If one occurrence is covariant and another
    contravariant, the joined variance is invariant, which correctly requires an
    equality constraint. Verify that this composition works correctly for our
    purposes.

1. **Recursive bounds**: Can a constraint like `T ≤ Sequence[T]` arise? If so,
    does it require special handling to avoid infinite loops during substitution?

1. **Circular dependencies in reverse decomposition**: The reverse direction
    invokes constraint set assignability from within sequent map construction.
    Need to verify this doesn't create cycles. May need a "raw" assignability
    check that doesn't consult the sequent map.

1. **DNF expansion for complex constraint set results**: When the assignability
    check produces a non-simple constraint set (e.g., a disjunction), we could
    view the BDD as a DNF and integrate each path's conjunction into
    `PathAssignments` by recursing with resets. Deferred for now.

1. **No natural Python example exercises forward direction today**: The forward
    direction (Phases 1–3) is correct and tested via mdtests, but currently no
    Python code path conjoins argument-level concrete bounds with callable-
    matching cross-typevar constraints into a single constraint set. This will
    be exercised once the specialization builder migrates to maintaining a single
    constraint set (tracked by the TODO in `add_type_mappings_from_constraint_set`).
