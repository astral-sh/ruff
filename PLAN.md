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

#### Step 1.1 ⬜ — Confirm that `variance_of` serves as our detection mechanism

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

#### Step 1.2 ⬜ — Add a `Single` variant to `ApplySpecialization`

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

#### Step 2.1 ⬜ — Add mdtest cases for covariant propagation (red)

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

#### Step 2.2 ⬜ — Add sequent generation for covariant nested typevars (green)

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

#### Step 2.3 ⬜ — Run the full test suite

Run the full constraint set and ty_python_semantic test suite to check for
regressions. Fix any issues before proceeding.

### Phase 3: Contravariant and invariant propagation

Once covariance is working end-to-end, extend the logic to handle the remaining
variance cases. These should be fairly straightforward modifications to the
covariance logic.

#### Step 3.1 ⬜ — Add mdtest cases for contravariant and invariant propagation (red)

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

#### Step 3.2 ⬜ — Add contravariant propagation

Extend the nested typevar sequent generation:

**`B` in `u_C` (upper bound of `C`), contravariant:**

- Emit `(l_B ≤ B ≤ u_B) ∧ (l_C ≤ C ≤ u_C) → (C ≤ u_C[B := l_B])`
    (The *lower* bound on `B` substitutes into the upper bound of `C` — flipped.)

**`B` in `l_C` (lower bound of `C`), contravariant:**

- Emit `(l_B ≤ B ≤ u_B) ∧ (l_C ≤ C ≤ u_C) → (l_C[B := u_B] ≤ C)`
    (The *upper* bound on `B` substitutes into the lower bound of `C` — flipped.)

#### Step 3.3 ⬜ — Add invariant propagation

Extend the nested typevar sequent generation:

- Only emit a derived constraint if `l_B = u_B` (equality constraint on `B`).
- If so, substitute that single type into either bound of `C`.

#### Step 3.4 ⬜ — Run the full test suite

Run the full test suite again to check for regressions.

### Phase 4: Handle the same-typevar case with nested references

#### Step 4.1 ⬜ — Extend `add_sequents_for_pair` for same-typevar with nested references

The dispatch in `add_sequents_for_pair` currently falls through to
`add_concrete_sequents` when both constraints are on the same typevar and
neither has a typevar as a direct bound. But a constraint like `T ≤ Sequence[T]`
(recursive bound) would have `T` nested in its upper bound while also directly
constraining `T`. Check whether this case actually arises in practice, and if
so, whether it needs special handling.

This is lower priority and may not need to be addressed initially.

### Phase 5: Clean up

#### Step 5.1 ⬜ — Remove or update the TODO comment

Remove the TODO comment at line 2828 of `constraints.rs` once the feature is
working. Update any related comments.

#### Step 5.2 ⬜ — Run `/home/dcreager/bin/jpk`

Final pre-commit checks. We are in a jj worktree, so use `jpk` (with no
arguments) as a standin for `prek`.

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
