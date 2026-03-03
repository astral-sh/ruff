# Constraint-set support tracking and `source_order` removal plan

## Goal

Replace per-interior-node `source_order` tracking with per-constraint-set **support expression** tracking.

The key design principle is:

- During BDD construction, support handling should be **very cheap** (record support operations only).
- Materialize concrete ordered support (`[ConstraintId]`) **lazily**, only when consumers need it.

This should:

1. Preserve stable, source-like ordering for diagnostics/solutions.
2. Avoid dependence on global/builder-local `ConstraintId` numeric order.
3. Enable future switch from quasi-reduced to fully reduced BDD nodes.
4. Shift CPU work out of hot BDD ops into rarer support-consumption paths.

---

## Scope and staging

- [ ] **Phase 1 (mechanical + behavior-preserving):**
  - Introduce support-expression infrastructure.
  - Thread support expressions through `ConstraintSet` operations.
  - Keep existing node `source_order` fields temporarily.
  - Add lazy flattening API and migrate consumers where practical.
- [ ] **Phase 2 (cleanup):**
  - Remove node-level `source_order`/`max_source_order`.
  - Remove offset machinery (`*_with_offset`, `with_adjusted_source_order`, etc.).
- [ ] **Phase 3 (optional follow-up):**
  - Evaluate switching from quasi-reduced to fully reduced BDDs.

This plan covers Phases 1 and 2.

---

## Data model changes

### 1) Add support-expression IDs and storage

In `crates/ty_python_semantic/src/types/constraints.rs`:

- Add new index type:
  - `#[newtype_index] struct UnionSupportId;`
- Add a support-expression identifier enum:

```rust
enum SupportId {
    Empty,
    Singleton(ConstraintId),
    OrderedUnion(UnionSupportId),
    // Quantification/abstraction support transform node (details below)
    Quantified(QuantifiedSupportId),
}
```

(Exact naming may vary; `SupportId` can remain module-private.)

```rust
struct UnionSupportData {
    lhs: SupportId,
    rhs: SupportId,
}
```

- Extend `ConstraintSetStorage` with:
  - `union_supports: IndexVec<UnionSupportId, UnionSupportData>`
  - `quantified_supports: IndexVec<QuantifiedSupportId, QuantifiedSupportData>`

Important initial policy:

- **Do not hash-cons any support-expression nodes** in Phase 1 (including `OrderedUnion` and `Quantified`/quantified nodes).
- Always append fresh support-expression nodes as operations are recorded.

### 2) Quantification/derived-support node

For existential abstraction and related flows, add a support node kind that records:

- base support expression,
- removed constraints,
- derived constraints tied to origin constraints (rank provenance),
- optional encounter index if needed to stabilize ties.

Proposed payload sketch:

```rust
struct QuantifiedSupportData {
    base: SupportId,
    removed: Box<[ConstraintId]>,
    derived: Box<[DerivedConstraintRecord]>,
}

struct DerivedConstraintRecord {
    origin: ConstraintId,
    derived: ConstraintId,
}
```

Note:

- We may later flatten/simplify `QuantifiedSupportData` and/or `DerivedConstraintRecord` if profiling or implementation experience suggests it.

Invariant:

- Every `origin` used in `derived` **must** appear in the flattened base support (after removals are interpreted per algorithm). If not, treat as programmer error and **hard panic**.

### 3) Make support expression explicit on `ConstraintSet`

- `ConstraintSet` gains `support: SupportId`.
- `OwnedConstraintSet` stores **materialized** support:
  - `support: Box<[ConstraintId]>`

`ConstraintSet::from_node` becomes `from_node_and_support(...)`.

---

## Support-expression operations

### 4) Builder helpers

Add methods on `ConstraintSetBuilder`:

- constructors:
  - `empty_support() -> SupportId`
  - `singleton_support(c: ConstraintId) -> SupportId`
  - `ordered_union_support(lhs: SupportId, rhs: SupportId) -> SupportId`
  - `quantified_support(data: QuantifiedSupportData) -> SupportId`

- flattening/materialization:
  - `flatten_support(expr: SupportId) -> Box<[ConstraintId]>`

`flatten_support` behavior:

- uses an `FxIndexSet<ConstraintId>` accumulator (type alias over `indexmap::IndexSet`),
- iterative traversal (no recursive stack growth risk),
- `OrderedUnion`: flatten lhs then rhs (graph structure defines ordering),
- dedupe comes from `FxIndexSet` insertion semantics (first occurrence wins),
- deterministic order,
- `Quantified`: apply explicit remove/derive operations over the accumulator.

Implementation notes:

- Keep flatten semantics centralized in this one method; each support node maps to a clear accumulator operation.
- `flatten_support` should rely on `FxIndexSet` for dedupe and order (no bespoke mark/epoch dedupe logic in this path).

### 5) Support creation rules

- Atomic single-constraint set: singleton support.
- `always` / `never`: empty support.
- `union` / `intersect` / `iff` / `implies`: record ordered-union support node (`lhs`, `rhs`) only.
- `negate`: support unchanged.
- existential/reduction operations:
  - record a `Quantified` support transform node; do not eagerly compute flattened order.

---

## Thread support through APIs

### 6) Constructors and conversions

Update:

- `ConstraintSet::always/never/from_bool/constrain_typevar/...`
- `ConstraintSetBuilder::into_owned` and `load`

Rules:

- `into_owned` materializes flattened support and stores `Box<[ConstraintId]>`.
- `load` remaps stored support constraints through remapped `ConstraintId`s and rebuilds support expression as a left-associated chain of singleton `OrderedUnion` nodes.

### 7) Combinators

Update all combinators to compose support expressions without flattening:

- `union`, `intersect`, `and`, `or`, `implies`, `iff`, `negate`
- quantification/restriction methods (`reduce_inferable`, `retain_non_inferable`, etc.)

Audit all `ConstraintSet::from_node(...)` sites and convert to support-aware constructor.

---

## Migrate consumers from `source_order` to flattened support rank

### 8) Path and solution ordering

Replace per-node `source_order` sorting with support-rank sorting:

- Build `constraint -> rank` map from `flatten_support(constraint_set.support)`.
- Sort positives by rank (stable; ties should not normally occur once rank map is built from flattened support).

Apply to:

- `NodeId::solutions`
- display/simplification code currently sorting by `source_order`

### 9) Simplification and quantification behavior

In abstraction/simplification flows:

- stop synthesizing/propagating node `source_order` values.
- represent derived-support semantics via `Quantified` support node.

For relative ordering among derived constraints from the same origin:

- use support-graph structure (construction order of support nodes) as the tie-breaker.
- do not add extra tie-break metadata initially.

### 10) `PathAssignments`

Current `FxIndexMap<ConstraintAssignment, usize>` may remain initially.

- reinterpret `usize` as support rank when populated from flattened support.
- do not assume rank originates from interior-node metadata.

---

## Remove `source_order` from nodes (Phase 2)

### 11) Interior node shape

Remove from `InteriorNodeData`:

- `source_order`
- `max_source_order`

Update `Node::new`, `new_constraint`, `new_satisfied_constraint` signatures.

### 12) Remove offset machinery

Delete/replace:

- `with_adjusted_source_order`
- `max_source_order`
- `or_with_offset`, `and_with_offset`, `iff_with_offset`
- cache key dimensions based on `other_offset`

Use normal binary ops at BDD layer; support ordering comes from support expression graph.

### 13) Update docs/comments

Replace references to node-level source-order with support-expression + flatten semantics.

---

## Fully reduced BDD follow-up (Phase 3)

### 14) Toggle reduction behavior

After support migration lands:

- evaluate removing quasi-reduction exception that preserves redundant nodes,
- benchmark and validate output stability.

Keep as separate PR/commit.

---

## Testing strategy

### 15) Unit tests in `constraints.rs`

Add tests for:

- flatten of ordered-union chains preserves lhs-first semantics,
- dedupe with first occurrence winning,
- deterministic flatten for deep trees,
- deterministic flatten behavior for large/deep support graphs,
- `Quantified` semantics (remove + derive + tie behavior),
- panic path when derived origin is missing,
- `into_owned`/`load` support remap.

### 16) Existing tests

Run focused tests first:

- `cargo nextest run -p ty_python_semantic`
- mdtests as needed:
  - `cargo nextest run -p ty_python_semantic --test mdtest -- mdtest::<path>`

Then broader checks per repo conventions.

---

## Performance validation

### 17) Instrumentation and sanity checks

Collect before/after metrics:

- support-expression node counts (`OrderedUnion`, `Quantified`),
- flattened support lengths at consumption sites,
- flatten invocation counts,
- wall time in representative mdtests/code-nav runs,
- memory impact from non-hash-consed support-expression nodes.

Implementation note:

- add lightweight temporary counters for this validation, and remove them from final landed code unless we decide to keep them as permanent diagnostics.

Expectation:

- lower CPU in BDD construction/hot combinators,
- potentially higher memory and flatter-consume cost.

---

## Risks and mitigations

1. **Support-expression tree growth (memory)**
   - Mitigation: intentional Phase-1 tradeoff; measure and revisit with optional memo/hash-consing later.
2. **Flatten correctness subtleties**
   - Mitigation: single `flatten_support` implementation + strong unit tests.
3. **Ordering drift in diagnostics/snapshots**
   - Mitigation: all ordering consumers rely on flattened rank map.
4. **Quantification provenance bugs**
   - Mitigation: explicit invariants; panic on missing origin; dedicated tests.
5. **Repeated flatten overhead in some paths**
   - Mitigation: measure with temporary counters; consider flatten memoization follow-up if needed.

---

## Concrete implementation checklist

- [ ] Add `SupportId`, `UnionSupportId`, `QuantifiedSupportId` and storage tables.
- [ ] Add support-expression constructors on builder (no support-node hash-consing).
- [ ] Add `flatten_support` with iterative traversal + `FxIndexSet` accumulator dedupe.
- [ ] Add `support` to `ConstraintSet` and flattened support payload to `OwnedConstraintSet`.
- [ ] Thread support expressions through constructors/combinators.
- [ ] Encode abstraction-derived ordering via `Quantified` support node.
- [ ] Convert ordering consumers to flattened support rank maps.
- [ ] Remove node `source_order` fields and offset APIs.
- [ ] Run tests and update snapshots if needed.

---

## Execution order with concrete code touchpoints

### Step A — add support-expression IDs and storage

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. Add `UnionSupportId` and `QuantifiedSupportId` (`#[newtype_index]`).
2. Add `SupportId` enum (`Empty`, `Singleton`, `OrderedUnion`, `Quantified`).
3. Extend `ConstraintSetStorage` with:
   - `union_supports: IndexVec<UnionSupportId, UnionSupportData>`
   - `quantified_supports: IndexVec<QuantifiedSupportId, QuantifiedSupportData>`
4. If needed, add lightweight scratch state for iterative flatten traversal (e.g., reusable explicit stack buffers).

### Step B — builder APIs and flatten implementation

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. Add support constructors (`empty`, `singleton`, `ordered_union`, `quantified`).
2. Implement `flatten_support(expr) -> Box<[ConstraintId]>`:
   - iterative walk,
   - `FxIndexSet` accumulator for dedupe/order,
   - deterministic lhs-before-rhs.
3. Implement clear per-node accumulator operations (`Empty`, `Singleton`, `OrderedUnion`, `Quantified`).
4. Add/adjust reusable traversal scratch only if profiling indicates allocation churn in flatten.

### Step C — thread support through structs and constructors

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. Add `support: SupportId` to `ConstraintSet`.
2. Add `support: Box<[ConstraintId]>` to `OwnedConstraintSet`.
3. Replace `from_node` with `from_node_and_support`.
4. Update `always`, `never`, `from_bool`, atomic constructors.

### Step D — update `into_owned` / `load`

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. `into_owned`: flatten and persist support list.
2. `load`: remap persisted support constraints and rebuild support expression.

### Step E — combinator updates

File: `crates/ty_python_semantic/src/types/constraints.rs`

Update first:

- `union`, `intersect`, `and`, `or`, `implies`, `iff`, `negate`

Rule:

- binary ops create `ordered_union_support(lhs.support, rhs.support)`
- unary negate keeps support unchanged.

Then audit all remaining `from_node(...)` sites.

### Step F — quantification support node wiring

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. In abstraction/reduction flows, build `Quantified` support nodes rather than eagerly computing ranks.
2. Record origins and derived constraints; rely on support-graph structure for relative ordering.
3. Enforce invariant that origins must be present at flatten time (hard panic otherwise).

### Step G — migrate ordering consumers

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. `NodeId::solutions` sorting.
2. `NodeId::path_assignments` ordering logic.
3. Any other display/simplify sorting based on `source_order`.

Pattern:

- flatten support once for the set,
- build rank map,
- sort by rank then `ConstraintId` fallback.

### Step H — remove source-order fields and offset APIs (Phase 2)

File: `crates/ty_python_semantic/src/types/constraints.rs`

1. Remove `source_order` / `max_source_order` from node data.
2. Remove `with_adjusted_source_order` and `*_with_offset` APIs + cache keys.
3. Update all call sites.

### Step I — tests and verification

1. Add/adjust support tests.
2. Run:
   - `cargo nextest run -p ty_python_semantic`
3. Snapshot accept if needed:
   - `cargo insta accept`

---

## Persisted context / handoff notes

### Confirmed design decisions

1. Support construction should be cheap; defer support calculation until needed.
2. No support-expression nodes are **hash-consed** in Phase 1 (`OrderedUnion` and `Quantified`/quantified included).
3. Flattening is centralized in builder (`flatten_support`) and uses an `FxIndexSet<ConstraintId>` accumulator.
4. `OwnedConstraintSet` persists flattened support, not support-expression graph.
5. `load` rebuilds support as a left-associated `OrderedUnion` chain of singleton nodes.
6. Missing origin for derived support is a programmer error (hard panic).
7. Relative ordering/tie-breaking comes from support-graph structure; no extra tie-break metadata initially.

### Invariants to preserve

For any materialized support list from `flatten_support`:

- each `ConstraintId` appears at most once,
- ordering is deterministic,
- ordered union is lhs-first,
- derived constraints honor origin-rank semantics.

### Suggested quick greps

- `rg -n "ConstraintSetBuilder|ConstraintSetStorage|ConstraintSet::from_node|OwnedConstraintSet|into_owned\(|load\(" crates/ty_python_semantic/src/types/constraints.rs`
- `rg -n "source_order|max_source_order|_with_offset|other_offset|with_adjusted_source_order" crates/ty_python_semantic/src/types/constraints.rs`
- `rg -n "solutions\(|path_assignments\(|positive_constraints\(" crates/ty_python_semantic/src/types/constraints.rs`

---

## Notes

- Keep semantic churn minimal in Phase 1: support-expression recording + lazy flatten.
- Keep fully reduced BDD work separate after migration stabilizes.
- Keep support-expression ID types private to `constraints.rs`.
- Consider flatten memoization only if profiling indicates repeated-flatten overhead.
