# Plan: Add "Uncertain" Branches to Constraint Set BDD (Ternary Decision Diagrams)

## Status markers

Each step will be marked with one of:

- `[ ]` — not started
- `[~]` — in progress
- `[x]` — complete

When resuming this plan, read through files in the repo to validate that the
status markers are accurate.

______________________________________________________________________

## Overview

We are extending the binary decision diagram (BDD) structure in
`crates/ty_python_semantic/src/types/constraints.rs` to a **ternary decision
diagram (TDD)**. Each interior node will gain a third outgoing edge called
`if_uncertain`, which represents the case where a constraint can be either true
or false — the result holds regardless.

**Semantics of a TDD node:**

```
⟦n ? C : U : D⟧ = (n ∩ ⟦C⟧) ∪ ⟦U⟧ ∪ (¬n ∩ ⟦D⟧)
```

Where:

- `n` = the constraint being tested
- `C` = `if_true` (constrained branch, taken when n holds)
- `U` = `if_uncertain` (included regardless of n)
- `D` = `if_false` (dual branch, taken when n does NOT hold)

**Rationale:** Unions become more efficient because the second operand can be
"parked" in the uncertain branch rather than duplicated into both the true and
false branches.

**Naming:** We keep our existing edge names (`if_true`, `if_false`) and add
`if_uncertain` as the third edge.

**Reference file:** `~/Downloads/duboc-tdd-summary.txt` — contains Frisch and
Duboc algorithms for union, intersection, and difference on TDDs.

We will implement **Set 2 (Duboc improved)** algorithms, with one correction:
the `n1 > n2` case for difference in Duboc's thesis is **incorrect** (it loses
the ¬U₂ restriction by moving U₂ from the right side of `\` to the left).
We use Frisch's original (Set 1) formula for that case. See the reference file
for a detailed explanation and counterexample.

______________________________________________________________________

## Phase 1: Data Structure Changes

### Step 1.1: Add `if_uncertain` field to `InteriorNodeData` [ ]

**File:** `constraints.rs`, struct `InteriorNodeData` (~line 2557)

Add a new field:

```rust
struct InteriorNodeData {
    constraint: ConstraintId,
    if_true: NodeId,
    if_uncertain: NodeId,   // NEW
    if_false: NodeId,
    source_order: usize,
    max_source_order: usize,
}
```

This will cause compilation errors everywhere `InteriorNodeData` is
constructed or destructured — those are addressed in subsequent steps.

### Step 1.2: Add `Unconstrained` variant to `ConstraintAssignment` [ ]

**File:** `constraints.rs`, enum `ConstraintAssignment` (~line 3628)

```rust
pub(crate) enum ConstraintAssignment {
    Positive(ConstraintId),
    Negative(ConstraintId),
    Unconstrained(ConstraintId),  // NEW
}
```

Update the existing methods on `ConstraintAssignment`:

- `constraint()`: Add arm for `Unconstrained(c) => c`
- `negated()`: `Unconstrained` returns itself — "this constraint can go either
    way" is symmetric under negation.
- `negate()`: Same as `negated()`.
- `implies()`: An `Unconstrained` assignment means "constraint can go either
    way." It does not imply any positive or negative assignment. Return `false`
    for all combinations involving `Unconstrained`, except
    `Unconstrained ⇒ Unconstrained` for the same constraint, which is trivially
    true.
- `display()`: Display as e.g. `(T =? int)` or `(T ≤? int)` to indicate
    uncertainty.

### Step 1.3: Update `NodeId::new` signature [ ]

**File:** `constraints.rs`, `NodeId::new` (~line 1387)

Change signature to accept `if_uncertain`:

```rust
fn new(
    builder: &ConstraintSetBuilder<'_>,
    constraint: ConstraintId,
    if_true: NodeId,
    if_uncertain: NodeId,   // NEW
    if_false: NodeId,
    source_order: usize,
) -> NodeId
```

Update the reduction rule: currently we reduce to `ALWAYS_FALSE` when both
`if_true` and `if_false` are `ALWAYS_FALSE`. Extend to require all three:

```rust
if if_true == ALWAYS_FALSE && if_uncertain == ALWAYS_FALSE && if_false == ALWAYS_FALSE {
    return ALWAYS_FALSE;
}
```

**General factoring principle:** Whenever we would create a node with all
three edges pointing to the same value `X`, prefer the factored form
`n ? 0 : X : 0`. Both are semantically equivalent under Duboc semantics
(`⟦n ? X : X : X⟧ = ⟦X⟧ = ⟦n ? 0 : X : 0⟧`), but the factored form keeps
TDDs compact. Note that when `X = ALWAYS_FALSE`, both forms reduce to
`ALWAYS_FALSE` via the existing reduction rule, so this only matters for
non-false values (primarily `ALWAYS_TRUE` in "remembering" nodes). Apply this
principle in `or_inner` terminal cases (Step 2.2), `new_satisfied_constraint`
for `Unconstrained` (Step 1.4), and anywhere else a node with three identical
edges would otherwise be constructed.

Update `max_source_order` calculation to include `if_uncertain`:

```rust
let max_source_order = source_order
    .max(if_true.max_source_order(builder))
    .max(if_uncertain.max_source_order(builder))
    .max(if_false.max_source_order(builder));
```

Update the debug assertions to also check `if_uncertain`'s root constraint
ordering.

### Step 1.4: Update `Node::new_constraint` and `Node::new_satisfied_constraint` [ ]

These create single-constraint BDD nodes.

`new_constraint`: The uncertain branch should be `ALWAYS_FALSE` — a single
constraint must be either true or false:

```rust
InteriorNodeData {
    constraint,
    if_true: ALWAYS_TRUE,
    if_uncertain: ALWAYS_FALSE,
    if_false: ALWAYS_FALSE,
    ...
}
```

`new_satisfied_constraint`: For `Positive` and `Negative` variants, use
`if_uncertain: ALWAYS_FALSE` as before. For the new `Unconstrained` variant,
the result holds regardless of the constraint's truth value, so only
`if_uncertain` should be `ALWAYS_TRUE` — the other two branches are
`ALWAYS_FALSE`. The Duboc algorithms maintain a factoring invariant where `C`
and `D` hold only what's *additionally* true beyond what `U` provides.
Since `Unconstrained` means everything is true regardless of `n`, it all
belongs in `U`:

```rust
ConstraintAssignment::Unconstrained(constraint) => {
    builder.intern_interior_node(InteriorNodeData {
        constraint,
        if_true: ALWAYS_FALSE,
        if_uncertain: ALWAYS_TRUE,
        if_false: ALWAYS_FALSE,
        source_order,
        max_source_order: source_order,
    })
}
```

(Note: `n ? 0 : 1 : 0` and `n ? 1 : 1 : 1` are semantically equivalent
under Duboc semantics — both evaluate to `1`. Overlap between `C`/`D` and
`U` is harmless since everything is unioned together; the factoring is an
efficiency concern (keeping TDDs compact), not a correctness one.)

### Step 1.5: Update memoization caches [ ]

The `node_cache: FxHashMap<InteriorNodeData, NodeId>` already uses
`InteriorNodeData` as the key. Since we're adding a field to `InteriorNodeData`,
this will automatically include `if_uncertain` in the hash/equality check. No
explicit change needed for interning correctness.

No new caches are needed at this point. The existing operation caches are keyed
on `(NodeId, NodeId, usize)` and remain valid since node IDs are unique.

### Step 1.6: Fix all compilation errors from struct changes [ ]

Every place that constructs an `InteriorNodeData` directly (bypassing
`NodeId::new`) must be updated to include `if_uncertain: ALWAYS_FALSE`. Search
for all occurrences of `InteriorNodeData {` in the file. Key locations:

- `intern_interior_node` — no change needed (receives `InteriorNodeData`)
- `Node::new_constraint` — Step 1.4
- `Node::new_satisfied_constraint` — Step 1.4
- `load` in `ConstraintSetBuilder` (~line 741) — Step 7.1

Every place that reads fields from `InteriorNodeData` must be updated to
handle `if_uncertain`. These will be addressed in the subsequent phases. As a
temporary measure to get compilation passing, you can add `let _ = interior.if_uncertain;` in places that aren't ready to handle it yet.

______________________________________________________________________

## Phase 2: Core Operation — Union (Duboc Set 2)

### Step 2.1: Update `InteriorNode::or` [ ]

**File:** `constraints.rs`, `InteriorNode::or` (~line 2600)

Implement the Duboc Set 2 union algorithm. The current binary BDD `or` has
three cases based on constraint ordering. The TDD version changes only the
`Less` and `Greater` cases (the `Equal` case naturally extends):

**n1 = n2 (Equal):**

```
n ? or(C1, C2) : or(U1, U2) : or(D1, D2)
```

This is a natural extension of the binary case — just add the uncertain branch.

**n1 < n2 (Less — self's constraint comes first):**

```
n1 ? C1 : or(U1, T2) : D1
```

Instead of duplicating T2 into both branches, park the entire T2 in the
uncertain branch (merged with U1). The `if_true` and `if_false` branches
keep only the self node's existing edges. This is the key efficiency gain.

**n1 > n2 (Greater — other's constraint comes first):**

```
n2 ? C2 : or(U2, T1) : D2
```

Symmetric to the Less case.

### Step 2.2: Update `NodeId::or_inner` terminal cases [ ]

**File:** `constraints.rs`, `NodeId::or_inner` (~line 1773)

Update the terminal-vs-interior cases:

- `(ALWAYS_TRUE, Interior(other))`: Create a "remembering" node using the
    factored form `n ? 0 : 1 : 0` (only `if_uncertain = ALWAYS_TRUE`), not
    `n ? 1 : 1 : 1`. Both are semantically equivalent, but the factored form
    is more compact and consistent with TDD conventions.
- `(Interior(self), ALWAYS_TRUE)`: Same, remembering self's constraint.
- `(ALWAYS_FALSE, _)` and `(_, ALWAYS_FALSE)`: Unchanged (identity element).

______________________________________________________________________

## Phase 3: Core Operation — Intersection (Duboc Set 2)

### Step 3.1: Update `InteriorNode::and` [ ]

**File:** `constraints.rs`, `InteriorNode::and` (~line 2651)

Implement the Duboc Set 2 intersection algorithm:

**n1 = n2 (Equal):**

```
n ? (C1 ∧ (C2 ∨ U2)) ∨ (U1 ∧ C2) : (U1 ∧ U2) : (D1 ∧ (U2 ∨ D2)) ∨ (U1 ∧ D2)
```

This is the key Duboc improvement over Frisch: the uncertain branch `U1 ∧ U2`
preserves partial laziness, instead of being zeroed out. The constrained and
dual branches are algebraically restructured to compensate.

**n1 < n2 (Less):**

```
n1 ? (C1 ∧ T2) : (U1 ∧ T2) : (D1 ∧ T2)
```

Distribute T2 into all three branches.

**n1 > n2 (Greater):**

```
n2 ? (T1 ∧ C2) : (T1 ∧ U2) : (T1 ∧ D2)
```

Symmetric.

### Step 3.2: Update `NodeId::and_inner` terminal cases [ ]

Update the terminal-vs-interior cases analogously to Step 2.2:

- `(ALWAYS_FALSE, Interior(other))`: All three edges `ALWAYS_FALSE`
    (remembering the constraint).
- `(ALWAYS_TRUE, _)`: Return other with adjusted source order (identity).

______________________________________________________________________

## Phase 4: Core Operation — Negation

### Step 4.1: Implement TDD negation as `1 \ T` (difference) [ ]

**File:** `constraints.rs`, `InteriorNode::negate` (~line 2577)

**Key insight:** TDD "leaf swap" negation (negate all three sub-TDDs
recursively) does NOT compute the set-theoretic complement under Duboc
semantics. The uncertain branch is *unioned* into both the true and false
interpretations, so its complement must be *intersected* — not unioned — back
in. Simple leaf swap gets this wrong.

Instead, define `negate(T) = 1 \ T` using the difference algorithm.
For `1 \ (n ? C : U : D)`, terminal `1` has no interior node, so we use the
`n1 > n2` case (Frisch/Set 1 — note that Duboc's Set 2 restructuring of
this case is incorrect; see the reference file):

```
n1 > n2:  n2 ? T1 \ (C2 ∨ U2) : 0 : T1 \ (D2 ∨ U2)
```

Substituting `T1 = 1`:

```
n ? 1 \ (C ∨ U) : 0 : 1 \ (D ∨ U)
= n ? negate(or(C, U)) : 0 : negate(or(D, U))
```

So the formula is:

```
negate(n ? C : U : D) = n ? negate(or(C, U)) : 0 : negate(or(D, U))
```

The uncertain branch `U` is first absorbed into `C` and `D` via union, and
then the combined branches are recursively negated. The result's uncertain
branch is always zero.

**Equivalent formulation** (derived algebraically from the complement):

```
negate(n ? C : U : D) = and(negate(U), n ? negate(C) : 0 : negate(D))
```

Both produce `¬⟦U⟧ ∩ (n ∩ ¬⟦C⟧ ∪ ¬n ∩ ¬⟦D⟧)`. Either can be used;
the first is directly derived from the difference algorithm, the second
separates the `¬U` intersection from the per-branch negation. Both are
correct.

**When U = ALWAYS_FALSE (the common case / backward compatibility):**

```
negate(n ? C : 0 : D)
  = n ? negate(or(C, 0)) : 0 : negate(or(D, 0))
  = n ? negate(C) : 0 : negate(D)
```

This is exactly the standard binary BDD leaf-swap negation with
`if_uncertain = 0`. There is **zero overhead** for existing BDDs that have
no uncertain branches.

**Important property:** `negate()` always produces a TDD where every node has
`if_uncertain = ALWAYS_FALSE`.

### Step 4.2: Update `NodeId::negate` terminal cases [ ]

Terminal cases remain unchanged:

- `negate(ALWAYS_TRUE) = ALWAYS_FALSE`
- `negate(ALWAYS_FALSE) = ALWAYS_TRUE`

______________________________________________________________________

## Phase 5: Core Operation — Iff (Biconditional)

### Step 5.1: Replace dedicated iff implementation with desugaring [ ]

**File:** `constraints.rs`

Frisch and Duboc do not describe a TDD `iff` operation. Rather than
maintaining a dedicated recursive implementation with its own cache, we
desugar `iff` into primitive operations — the same pattern already used
for `implies`.

Remove the following:

- `InteriorNode::iff`
- `iff_cache` from `ConstraintSetStorage`

Replace `NodeId::iff_with_offset`, `NodeId::iff`, and `NodeId::iff_inner`
with simple helpers that desugar into `and`/`or`/`negate`:

```rust
fn iff(self, builder: &ConstraintSetBuilder<'_>, other: Self) -> Self {
    // iff(a, b) = (a ∧ b) ∨ (¬a ∧ ¬b)
    let a_and_b = self.and(builder, other);
    let not_a_and_not_b = self.negate(builder).and(builder, other.negate(builder));
    a_and_b.or(builder, not_a_and_not_b)
}
```

Keep `NodeId::iff` and `NodeId::iff_with_offset` as convenience helpers
(there are callers at the `NodeId` level in `satisfied_by_all_typevars`
and the substitution methods), but they no longer need their own recursive
implementation or cache. The existing caches for `negate`, `and`, and `or`
handle memoization.

Since `negate()` produces flat TDDs (all uncertain=0), the `¬a ∧ ¬b`
intersection uses the simpler binary-like code path.

______________________________________________________________________

## Phase 6: Update Derived Operations

### Step 6.1: Update `ite` (if-then-else) [ ]

**File:** `constraints.rs`, `NodeId::ite` (~line 1987)

Currently defined as:

```rust
fn ite(self, builder, then_node, else_node) -> Self {
    self.and(builder, then_node)
        .or(builder, self.negate(builder).and(builder, else_node))
}
```

This definition still works correctly with the updated `and`, `or`, and
`negate` operations. No changes needed to the definition itself, but verify
it produces correct results for TDD inputs.

### Step 6.2: Update `implies` [ ]

Currently defined as `self.negate(builder).or(builder, other)`. This still
works correctly. No changes needed.

### Step 6.3: Update `restrict_one` [ ]

**File:** `constraints.rs`, `InteriorNode::restrict_one` (~line 2917)

`restrict` fixes a BDD variable to a specific value and removes it. For TDDs,
when a constraint is fixed, the uncertain branch must be folded in:

- `restrict(n ? C : U : D, n.true)` → `or(C, U)` — because if n holds, we
    get the true branch *plus* the uncertain branch
- `restrict(n ? C : U : D, n.false)` → `or(D, U)` — similarly
- `restrict(n ? C : U : D, n.unconstrained)` → `or(C, U, D)` — the
    constraint can go either way, so we get everything reachable from any
    branch
- For a non-matching constraint, recurse into all three branches:
    `n ? restrict(C, m) : restrict(U, m) : restrict(D, m)`

### Step 6.4: Update `exists_one` / `abstract_one_inner` [ ]

**File:** `constraints.rs`, `InteriorNode::exists_one` (~line 2729),
`InteriorNode::abstract_one_inner` (~line 2778)

Existential abstraction removes a typevar from the constraint set. When a
node's constraint is being removed, the result is `or(if_true, if_uncertain, if_false)` — i.e., the TDD is satisfied if *any* of the three branches is
satisfied.

When a node's constraint is NOT being removed, recurse into all three branches:

```rust
let if_uncertain = path.walk_edge(
    db, builder,
    self_interior.constraint.when_unconstrained(),  // NEW
    self_interior.source_order,
    |path, _| {
        self_interior.if_uncertain.abstract_one_inner(db, builder, should_remove, path)
    },
).unwrap_or(ALWAYS_FALSE);
```

Add a `when_unconstrained()` method to `ConstraintId`:

```rust
fn when_unconstrained(self) -> ConstraintAssignment {
    ConstraintAssignment::Unconstrained(self)
}
```

### Step 6.5: Update `with_adjusted_source_order` [ ]

**File:** `constraints.rs`, `NodeId::with_adjusted_source_order` (~line 1509)

Recurse into `if_uncertain` as well:

```rust
Node::Interior(_) => {
    let interior = builder.interior_node_data(self);
    NodeId::new(
        builder,
        interior.constraint,
        interior.if_true.with_adjusted_source_order(builder, delta),
        interior.if_uncertain.with_adjusted_source_order(builder, delta),
        interior.if_false.with_adjusted_source_order(builder, delta),
        interior.source_order + delta,
    )
}
```

### Step 6.6: Update `for_each_constraint` [ ]

**File:** `constraints.rs`, `NodeId::for_each_constraint` (~line 2313)

Also traverse `if_uncertain`:

```rust
interior.if_true.for_each_constraint(builder, f);
interior.if_uncertain.for_each_constraint(builder, f);
interior.if_false.for_each_constraint(builder, f);
```

______________________________________________________________________

## Phase 7: Update BDD Walking and Path Analysis

### Step 7.1: Update `for_each_path` / `for_each_path_inner` [ ]

**File:** `constraints.rs`, `NodeId::for_each_path_inner` (~line 1546)

When walking the uncertain branch, **expand** the `Unconstrained` assignment
into both `Positive` and `Negative` alternatives. This way, the callback `f`
only ever sees paths with fully-determined (positive/negative) assignments,
and downstream consumers like `solutions` don't need any changes to handle
`Unconstrained`.

Collapse into two `walk_edge` calls — one for positive, one for negative —
and recurse into both `if_true`/`if_uncertain` (or `if_false`/`if_uncertain`)
inside each callback:

```rust
Node::Interior(_) => {
    let interior = builder.interior_node_data(self);
    // Positive: walk if_true and if_uncertain
    path.walk_edge(
        db, builder,
        interior.constraint.when_true(),
        interior.source_order,
        |path, _| {
            interior.if_true.for_each_path_inner(db, builder, f, path);
            interior.if_uncertain.for_each_path_inner(db, builder, f, path);
        },
    );
    // Negative: walk if_false and if_uncertain
    path.walk_edge(
        db, builder,
        interior.constraint.when_false(),
        interior.source_order,
        |path, _| {
            interior.if_false.for_each_path_inner(db, builder, f, path);
            interior.if_uncertain.for_each_path_inner(db, builder, f, path);
        },
    );
}
```

This mirrors the TDD semantics directly: when the constraint holds, the
result is `⟦C⟧ ∪ ⟦U⟧`; when it doesn't, the result is `⟦D⟧ ∪ ⟦U⟧`.
Paths through `if_uncertain` are naturally explored under both assignments,
and the sequent map handles any pruning of impossible paths.

### Step 7.2: Update `is_always_satisfied_inner` [ ]

**File:** `constraints.rs`, `NodeId::is_always_satisfied_inner` (~line 1589)

Add a check for the uncertain branch. The TDD is always satisfied only if
all three branches are always satisfied. Walk the uncertain branch with an
`Unconstrained` assignment (no expansion needed — we're checking a boolean
property, not enumerating solutions):

```rust
// Check uncertain branch
let uncertain_always_satisfied = path
    .walk_edge(
        db, builder,
        interior.constraint.when_unconstrained(),
        interior.source_order,
        |path, _| {
            interior.if_uncertain.is_always_satisfied_inner(db, builder, path)
        },
    )
    .unwrap_or(true);
if !uncertain_always_satisfied {
    return false;
}
```

### Step 7.3: Update `is_never_satisfied_inner` [ ]

**File:** `constraints.rs`, `NodeId::is_never_satisfied_inner` (~line 1643)

Same pattern — the TDD is never satisfied only if all three branches are
never satisfied. Walk the uncertain branch with `when_unconstrained()`.

### Step 7.4: Update `PathAssignments::walk_edge` for Unconstrained [ ]

**File:** `constraints.rs`, `PathAssignments::walk_edge` (~line 4545) and
`PathAssignments::add_assignment` (~line 4650)

The `walk_edge` method calls `add_assignment` which checks for contradictions.
We need to define what an `Unconstrained` assignment means in this context:

`Unconstrained(c)` is semantically equivalent to `c ∨ ¬c`, which is `true`.
The assignments on a path are AND'd together, and AND-ing `true` doesn't
change the result. This means:

- If there is already a `Positive(c)` or `Negative(c)` assignment on the
    path, the `Unconstrained(c)` assignment is redundant and should NOT be
    added — the existing assignment is strictly more informative.
- If there is no existing assignment for `c`, add the `Unconstrained(c)`
    assignment to record that we walked the uncertain branch for this
    constraint.
- `Unconstrained` assignments should NOT trigger sequent-based inference,
    because they don't assert anything about the constraint's truth value.

Implementation: in `add_assignment`, when receiving an `Unconstrained`
assignment, first check if there is already ANY assignment for that
constraint (positive, negative, or unconstrained). If so, return early
(no-op). Otherwise, add it to the assignments map and skip all sequent-based
inference (no tautology checks, no impossibility checks, no implication
propagation).

### Step 7.5: No changes needed to `PathAssignments` accessors [ ]

The existing `positive_constraints` accessor remains correct. Callers that
need to enumerate solutions (Step 8.1) will only ever see `Positive` and
`Negative` assignments, because `for_each_path` expands `Unconstrained`
assignments into both alternatives (see Step 7.1).

### Step 7.6: Update `PathAssignments::assignment_holds` [ ]

This checks if a specific assignment is in the current path. An
`Unconstrained` assignment should only match itself. The existing
`contains_key` lookup handles this correctly since `ConstraintAssignment`
derives `Eq`/`Hash` and the new variant will be distinct.

______________________________________________________________________

## Phase 8: Update Solutions, Display, and Simplification

### Step 8.1: Update `InteriorNode::solutions` [ ]

**File:** `constraints.rs`, `InteriorNode::solutions` (~line 2884)

No significant changes needed. The solutions method walks all paths via
`for_each_path`, which now expands uncertain branches into both positive
and negative alternatives (Step 7.1). This means `for_each_path` only
delivers paths with `Positive` and `Negative` assignments — the existing
`positive_constraints` accessor and solution extraction logic work as-is.

Verify that the expansion in `for_each_path` produces correct and
non-redundant solutions. The sequent map / path assignment deduplication
should handle most overlap between the `if_true` paths and the positive
expansion of `if_uncertain`, but confirm this empirically with tests.

### Step 8.2: Update `satisfied_clauses` [ ]

**File:** `constraints.rs`, `NodeId::satisfied_clauses` (~line 2341)

The `Searcher` visits all paths to `ALWAYS_TRUE`. Update to also visit the
`if_uncertain` edge:

```rust
Node::Interior(_) => {
    let interior = builder.interior_node_data(node);
    self.current_clause.push(interior.constraint.when_true());
    self.visit_node(builder, interior.if_true);
    self.current_clause.pop();
    self.current_clause.push(interior.constraint.when_unconstrained());
    self.visit_node(builder, interior.if_uncertain);
    self.current_clause.pop();
    self.current_clause.push(interior.constraint.when_false());
    self.visit_node(builder, interior.if_false);
    self.current_clause.pop();
}
```

### Step 8.3: Update `display_graph` [ ]

**File:** `constraints.rs`, `NodeId::display_graph` (~line 2416)

Add a third branch to the tree display. Proposed format:

```text
<0> (T = int) 1/1
┡━₁ always
├─? never
└─₀ never
```

Where `├─?` is the uncertain branch (or use `┝━?`). This lets the graph
visualization clearly show all three outgoing edges.

### Step 8.4: Update `SatisfiedClause::display` and `SatisfiedClauses::display` [ ]

`satisfied_clauses` (Step 8.2) DOES produce `Unconstrained` assignments
(unlike `for_each_path`, which expands them). `SatisfiedClause::display`
needs to handle the new variant.

The current `SatisfiedClause::display` contains duplicated logic for
formatting constraint assignments. Simplify it to delegate to
`ConstraintAssignment::display` (which already handles all variants
including the new `Unconstrained` display format from Step 1.3). This
removes the stale duplication and handles the new variant in one go.

### Step 8.5: Update `simplify_for_display` [ ]

**File:** `constraints.rs`, `InteriorNode::simplify` (~line 2851)

This method walks all constraints in the BDD and looks for simplification
opportunities. Update `for_each_constraint` traversal (Step 6.6) to include
uncertain branches. The simplification logic itself should work on the
semantic content (constraint pairs), which doesn't change — but the
substitution methods called within need to handle ternary nodes.

### Step 8.6: Update `substitute_intersection` and `substitute_union` [ ]

**File:** `constraints.rs`, `NodeId::substitute_intersection` (~line 2183),
`NodeId::substitute_union` (~line 2244)

These methods use `restrict` and `ite` to perform substitutions. Since
`restrict` is updated (Step 6.3) and `ite` works compositionally (Step 6.1),
these methods should produce correct results.

However, verify carefully:

- `restrict` now returns `or(branch, uncertain)` when fixing a variable, so the
    Shannon expansion values (`when_left_and_right`, `when_not_left`, etc.) will
    include the uncertain components.
- `ite` constructs a result using `and`/`or`/`negate`, all of which are
    TDD-aware.

Test these methods thoroughly to ensure the substitutions remain consistent.

______________________________________________________________________

## Phase 9: Update OwnedConstraintSet and Builder Load

### Step 9.1: Update `OwnedConstraintSet` [ ]

**File:** `constraints.rs`, struct `OwnedConstraintSet` (~line 173)

The `nodes: IndexVec<NodeId, InteriorNodeData>` field will automatically pick
up the new `if_uncertain` field. No explicit changes needed to the struct.

### Step 9.2: Update `ConstraintSetBuilder::load` [ ]

**File:** `constraints.rs`, `rebuild_node` in `load` (~line 741)

Currently rebuilds nodes using `condition.ite(builder, if_true, if_false)`.
This needs to also rebuild `if_uncertain`:

```rust
let if_true = rebuild_node(db, builder, other, cache, old_interior.if_true);
let if_uncertain = rebuild_node(db, builder, other, cache, old_interior.if_uncertain);
let if_false = rebuild_node(db, builder, other, cache, old_interior.if_false);
```

We cannot replace the `ite` call with a direct `NodeId::new`, since there
is no guarantee that the constraints will be ordered in the new builder the
same as in the old. Two options:

**Option A: 4-argument `ite` variant.** Add an `ite_uncertain` (or similar)
that takes `if_true`, `if_uncertain`, and `if_false` edges and constructs
a ternary node via the same ordering-aware logic that `ite` uses:

```rust
let remapped = condition.ite_uncertain(builder, if_true, if_uncertain, if_false);
```

**Option B: Absorb uncertain into true/false and use existing 3-arg `ite`.**
OR the uncertain edge into both the true and false edges, collapsing back
to a binary node. This loses the TDD laziness but is correct:

```rust
let if_true_merged = if_true.or(builder, if_uncertain);
let if_false_merged = if_false.or(builder, if_uncertain);
let remapped = condition.ite(builder, if_true_merged, if_false_merged);
```

**Recommendation:** Option B is likely sufficient — `load` is only used for
`OwnedConstraintSet` in mdtests, where efficiency is less of a concern.
Add a `TODO` comment in the code noting that this collapses the uncertain
branch and that a 4-arg `ite_uncertain` could preserve TDD structure if
`load` ever becomes performance-sensitive.

### Step 9.3: Update `ConstraintSetBuilder::into_owned` [ ]

No changes needed — it extracts constraints and nodes from the storage, which
will automatically include the new `if_uncertain` field.

______________________________________________________________________

## Phase 10: Update `satisfied_by_all_typevars`

### Step 10.1: Verify correctness [ ]

**File:** `constraints.rs`, `NodeId::satisfied_by_all_typevars` (~line 1730)

This method uses `implies`, `iff`, `and`, and `is_never/always_satisfied` —
all of which will be updated. Verify that the logic remains correct:

- For inferable typevars: "some valid specialization satisfies" — the
    `implies` + `and` construction should work with TDDs.
- For non-inferable typevars: "all required specializations satisfy" — the
    `iff` + `is_always_satisfied` check should work.

The `valid_specializations` and `required_specializations` methods on
`BoundTypeVarInstance` construct constraint nodes with `Constraint::new_node`,
which always produces nodes with `if_uncertain = ALWAYS_FALSE`. These should
remain correct.

______________________________________________________________________

## Phase 11: Backward Compatibility Verification

### Step 11.1: Verify that existing BDDs (uncertain=0) produce identical results [ ]

All existing code constructs BDD nodes with `if_uncertain = ALWAYS_FALSE`.
Verify algebraically that the Duboc algorithms degenerate to the current
binary BDD algorithms when all uncertain branches are zero:

**Union (n1=n2):** `or(C1,C2) : or(0,0) : or(D1,D2)` = `or(C1,C2) : 0 : or(D1,D2)` ≡ binary `or(C1,C2) : or(D1,D2)` ✓

**Union (n1\<n2):** `C1 : or(0,T2) : D1` = `C1 : T2 : D1` — semantically
`(n1 ∩ C1) ∪ T2 ∪ (¬n1 ∩ D1)` = `(n1 ∩ (C1 ∪ T2)) ∪ (¬n1 ∩ (D1 ∪ T2))`
= binary `(C1 ∨ T2) : (D1 ∨ T2)` ✓

**Intersection (n1=n2):** With U1=U2=0: `(C1∧C2) ∨ 0 : 0 : (D1∧D2) ∨ 0`
= `C1∧C2 : 0 : D1∧D2` ≡ binary `C1∧C2 : D1∧D2` ✓

**Negation:** With U=0: `and(negate(0), n ? negate(C) : 0 : negate(D))`
= `and(1, n ? negate(C) : 0 : negate(D))` = `n ? negate(C) : 0 : negate(D)`
≡ binary leaf swap ✓

This verification should be done analytically (confirm the math) and
empirically (run the full test suite and confirm no changes in output).

### Step 11.2: Run the full test suite [ ]

```sh
cargo nextest run -p ty_python_semantic
cargo nextest run -p ty_python_semantic --test mdtest -- mdtest::type_properties/constraints.md
```

Ensure all existing tests pass without modification. Since we're only adding
a field defaulting to `ALWAYS_FALSE` and the algorithms degenerate correctly,
existing tests should be unaffected.

______________________________________________________________________

## Phase 12: Testing the New Functionality

### Step 12.1: Add unit tests for TDD operations [ ]

Add tests to the `tests` module at the bottom of `constraints.rs`:

1. **Union laziness test**: Verify that `or(T1, T2)` where T1 and T2 have
    different root constraints produces a node with a non-ALWAYS_FALSE uncertain
    branch (rather than duplicating T2 into both branches).

1. **Intersection preservation test**: Verify that `and(T1, T2)` where both
    have uncertain branches preserves `U1 ∧ U2` in the uncertain branch (the
    Duboc improvement).

1. **Negation correctness test**: Verify that `negate(negate(T)) ≡ T`
    semantically (the result should be `is_always_satisfied` iff the original
    is). Also verify `and(T, negate(T))` is never satisfied, and
    `or(T, negate(T))` is always satisfied.

1. **Iff correctness test**: Verify `iff(T, T)` is always satisfied, and
    `iff(T, negate(T))` is never satisfied.

1. **Round-trip test**: Build a TDD with uncertain branches, convert to
    `OwnedConstraintSet`, load into a new builder, and verify the result is
    semantically equivalent.

### Step 12.2: Add mdtest cases [ ]

Add test cases to `resources/mdtest/type_properties/constraints.md` that
exercise scenarios where TDD uncertain branches provide more efficient
representations, and verify that the constraint solver still produces correct
results.

______________________________________________________________________

## Key Design Decisions and Open Questions

### Decision: Negation strategy

We define `negate(T) = 1 \ T` using the difference algorithm (Frisch/Set 1
for the `n1 > n2` case — Duboc's Set 2 restructuring of that case is
incorrect). This yields:
`negate(n ? C : U : D) = n ? negate(or(C, U)) : 0 : negate(or(D, U))`.
The uncertain branch is absorbed into C and D via union before negation.
The result always produces flat TDDs (all uncertain=0) and has zero overhead
when the input is already flat — it degenerates to standard binary BDD
leaf-swap negation.

### Decision: Iff strategy

We desugar `iff(a, b) = or(and(a, b), and(negate(a), negate(b)))` at the
`ConstraintSet` level, removing the dedicated `NodeId`/`InteriorNode`
methods and `iff_cache`. This follows the same pattern as `implies`. The
existing caches for `and`, `or`, and `negate` handle memoization. Since
negate produces flat TDDs, the `and(negate(a), negate(b))` term uses the
simpler binary-like intersection code path.

### Decision: Restrict semantics

`restrict(T, n.true) = or(C, U)`, `restrict(T, n.false) = or(D, U)`, and
`restrict(T, n.unconstrained) = or(C, U, D)`.

### Open question: SequentMap interactions with Unconstrained

Sequent maps track derived facts about constraint relationships. An
`Unconstrained` assignment provides no information, so it should not trigger
any sequent inference. This is handled in Step 7.4. Note that
`for_each_path` and friends expand uncertain branches into positive/negative
alternatives (Step 7.1), so most path-walking code never sees
`Unconstrained` assignments at all.

### Open question: `is_cyclic` with uncertain branches

The `is_cyclic` method walks constraints via `for_each_constraint`. Since
Step 6.6 updates that to traverse uncertain branches, cycle detection should
automatically cover constraints that only appear in uncertain branches.

______________________________________________________________________

## Execution Order

The phases are designed to be executed in order. Within each phase, steps
should be done sequentially. The dependency graph is:

```
Phase 1 (data structures)
  ├── Phase 2 (union)
  ├── Phase 3 (intersection)
  ├── Phase 4 (negation) ← depends on Phase 3 (uses `and`)
  ├── Phase 5 (iff) ← depends on Phases 2, 3, 4
  ├── Phase 6 (derived ops) ← depends on Phases 2-5
  │   └── Phase 7 (path walking) ← depends on Phase 6
  │       └── Phase 8 (solutions/display) ← depends on Phase 7
  ├── Phase 9 (owned/load) ← depends on Phase 1
  ├── Phase 10 (verify typevars) ← depends on Phases 2-7
  └── Phase 11 (backward compat) ← depends on all above
      └── Phase 12 (new tests)
```

After each phase, run `jpk` (jj worktree — do NOT use `uvx prek`) and
`cargo nextest run -p ty_python_semantic` to catch issues early.
