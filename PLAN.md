# Plan: Garbage collect `OwnedConstraintSet` storage

## Working instructions for future agents

- Treat this file as the ground truth for ordering and status. Update the status markers as work is completed or the plan changes.
- Before editing code, create a new `jj` revision with `jj new`/`jj new -A @` and describe it with a `[π]`-prefixed description. Do not use `git` directly.
- Use `$HOME/.pi/tmp` rather than `/tmp` for persistent temporary files.
- When working on ty changes, read `.agents/skills/working-on-ty/SKILL.md` and follow any more-specific skill it points to.
- After changing files, run the targeted tests described below, review any generated snapshots, and run prek via `/home/dcreager/bin/jpk` for changed files in this `jj` worktree.

## Status

- [x] Read the relevant constraint-set and reachability-constraint implementations.
- [x] Write the initial implementation plan.
- [x] Decision: compact `nodes` and `constraints` using rank mappings, without rewriting retained IDs.
- [x] Decision: leave `typevars` dense/uncompacted because that arena is usually small, and retaining it avoids extra typevar-marking complexity.
- [x] Implement compacted owned storage with query-builder overlays.
- [x] Add tests that exercise compacted owned sets via both `query` and `load`.
- [x] Run targeted tests and prek.

## Goal

`ConstraintSetBuilder::into_owned` currently stores every `nodes` and `constraints` arena entry that was ever interned in the builder, even though only one `ConstraintSet` is extracted and only storage reachable from that set's root can ever be observed. This wastes Salsa-cache memory for `OwnedConstraintSet` values.

Implement marking/filtering passes, modeled on `crates/ty_python_core/src/reachability_constraints.rs`, so that `OwnedConstraintSet` retains only the nodes and constraints needed by its root. Do not rewrite IDs inside retained data; instead, keep `RankBitBox` mappings from old IDs to positions in compacted storage for `NodeId` and `ConstraintId`. Leave `TypeVarId` storage dense/uncompacted.

## Relevant current code

- Main implementation: `crates/ty_python_semantic/src/types/constraints.rs`.
    - `OwnedConstraintSet` currently stores `node`, full `constraints`, full `typevars`, and full `nodes` arenas.
    - `ConstraintSetBuilder::into_owned` consumes the builder, shrinks the full arenas, and moves them into `OwnedConstraintSet` without dropping unreachable entries.
    - `OwnedConstraintSet::query` builds a temporary `ConstraintSetBuilder` by cloning the owned arenas and rebuilding arena caches.
    - `ConstraintSetBuilder::load` rebuilds an owned set into an existing builder by recursively reading `other.nodes[old_node]` and iterating `other.constraints`.
    - `ConstraintSetBuilder::interior_node_data` and `ConstraintSetBuilder::constraint_data` are central read paths for interior-node and constraint data inside a builder.
    - `ConstraintSetBuilder::typevar_id` reads `typevar_cache`; overlaid query builders must preserve old `TypeVarId`s when the typevar lives in `OwnedConstraintSetInner` and must offset newly interned local typevars.
    - `ConstraintId::ordering` and `BoundTypeVarInstance::can_be_bound_for` use numeric ID indexes for ordering. Filtering preserves relative ordering of retained entries as long as old IDs remain visible through rank mappings.
- Model implementation: `crates/ty_python_core/src/reachability_constraints.rs`.
    - `ReachabilityConstraintsBuilder::mark_used` recursively marks reachable TDD nodes.
    - `ReachabilityConstraintsBuilder::build` filters the arena to `used_interiors` and stores `used_indices: Option<Box<RankBitBox>>` when compaction occurred.
    - `ReachabilityConstraints::get_interior_node` maps an old ID through `RankBitBox::rank` before indexing compact storage.
- `RankBitBox` lives in `crates/ty_python_core/src/rank.rs`; `ty_python_semantic` already depends on `ty_python_core`.

## Decision log

- **Compact nodes and constraints; keep typevars dense.**

    - Decision: compact `nodes` and `constraints`, but leave `typevars` unchanged.
    - Rationale: compaction filters storage without reordering retained entries, and rank mappings let retained node data continue storing old `NodeId`s and `ConstraintId`s. `typevars` is expected to be the smallest arena, and not compacting it avoids needing a separate policy for typevars mentioned only in retained constraint bounds.

- **Use `RankBitBox::len()` as the overlay split point.**

    - Decision: add a `len()` accessor to `RankBitBox` and use it to determine the first new ID after compaction. For both compacted arenas, truncate the bitbox to one past the highest retained ID instead of covering the full original arena length.
    - Rationale: new IDs only need to be greater than every retained old ID. Retained interior nodes cannot refer to higher filtered-out nodes because nodes are constructed from already-existing child IDs, and retained nodes do not mention filtered-out constraints. This lets us avoid preserving unused high-ID tail entries in the rank bitboxes.

- **Use overlaid compacted storage in query builders.**

    - Decision: keep `ConstraintSetStorage` as mutable storage, but add an optional overlay pointing at an owned set's compacted storage. Old IDs below the overlay split point read from the compacted overlay via rank mapping; new IDs at or above that split point read from the normal dense arenas after subtracting the split point.
    - Rationale: this avoids turning `ConstraintSetStorage` into an enum and avoids filling dense arenas with dummy entries for filtered-out IDs. Query builders can allocate new nodes/constraints in small dense arenas while preserving old IDs for the compacted owned data.

- **Store owned storage behind one optional `Arc<OwnedConstraintSetInner>`.**

    - Decision: make `OwnedConstraintSet` hold the root `NodeId` plus `Option<Arc<OwnedConstraintSetInner<'db>>>`. The inner value, when present, holds compacted node and constraint storage plus their rank mappings, and the uncompacted typevar arena.
    - Rationale: query builders can move/clone one handle into their optional compacted-storage overlay. This avoids per-arena `Arc` fields, keeps the overlay representation simple, and avoids allocating storage for terminal roots.

- **Build overlay identity caches lazily as a group.**

    - Decision: build `typevar_cache`, `constraint_cache`, and `node_cache` lazily from overlay entries. Any `intern_*` method (`intern_typevar`, `intern_constraint`, or `intern_interior_node`) should trigger population of all three identity caches before lookup, local allocation, or insertion.
    - Rationale: read-only queries avoid identity-cache work, while any mutating interning path still preserves the invariant that interning data already present in the overlay returns the old ID instead of allocating a duplicate.

- **Overlay typevars without compacting them.**

    - Decision: keep the full `IndexVec<TypeVarId, BoundTypeVarIdentity<'db>>` in `OwnedConstraintSetInner` and do not clone it into `ConstraintSetStorage::typevars` for query builders. When an overlay is present, `ConstraintSetBuilder` should split typevar lookup/allocation at `inner.typevars.len()` without a `RankBitBox`: old `TypeVarId`s below the split read from `inner.typevars`; new typevars are stored in the local dense `typevars` arena with the split point added to their returned IDs.
    - Rationale: typevars are not compacted, so the split is a simple length check. This avoids cloning the typevar arena while still keeping typevar IDs overlay-oblivious to callers.

- **Retain only constraints referenced by retained nodes.**

    - Decision: compact the constraint arena to constraints referenced by reachable `InteriorNodeData::constraint` fields.
    - Rationale: operation caches are not retained in owned sets, and typevars remain dense. If no retained node references a constraint, the owned TDD cannot observe it.

- **Keep the root node on the outer owned set, with optional shared storage.**

    - Decision: use `OwnedConstraintSet { node: NodeId, inner: Option<Arc<OwnedConstraintSetInner<'db>>> }`, with `OwnedConstraintSetInner` containing only storage arenas and rank mappings when storage is present.
    - Rationale: this matches the current shape, keeps query overlays focused on storage, leaves room for possible future sharing of one inner storage object by multiple roots, and avoids allocating an empty inner object for storage-free owned sets.

- **Discard storage for terminal owned sets.**

    - Decision: when `into_owned` extracts a terminal root, set `inner: None`, even if the builder had unused nodes, constraints, or typevars. For nonterminal roots, create `inner: Some(...)`.
    - Rationale: with a terminal root, no retained TDD data is semantically observable by the owned set. For nonterminal roots, reachable-node marking should retain the needed nodes and constraints.

- **Use no overlay when owned storage is absent.**

    - Decision: when `OwnedConstraintSet::inner` is `None`, `query` constructs a normal empty `ConstraintSetBuilder` with no compacted overlay.
    - Rationale: this falls out naturally from optional-overlay storage and lets mutating callbacks allocate from ID 0, equivalent to querying a freshly created terminal set.

- **Fail loudly for filtered-out below-split IDs.**

    - Decision: if an accessor receives a `NodeId` or `ConstraintId` below the overlay split point whose rank-bit is unset, treat it as an invariant violation by using `expect`/panic in the lookup path. No separate `debug_assert!` or `assert!` is needed if the code converts both an out-of-range bit lookup and an unset bit into the `expect` failure path.
    - Rationale: retained data should never refer to filtered-out entries, and new IDs start at the split point. Returning local dense data would be ambiguous and silently wrong.

- **Expose set-bit iteration from `RankBitBox`.**

    - Decision: add `RankBitBox::iter_ones()` to delegate to the underlying bitvec helper and yield retained old indexes.
    - Rationale: lazy overlay cache construction needs retained old IDs. Exposing set-bit iteration avoids storing persistent old-ID arrays in `OwnedConstraintSetInner` and avoids implementing full `select`.

- **Always use the inner/overlay representation for nonterminal roots.**

    - Decision: whenever an owned set has a nonterminal root, create `OwnedConstraintSetInner` with rank bitboxes and query through the overlay path, even if all entries in the retained prefix are used.
    - Rationale: one representation for nonterminal owned storage is simpler, and skipping rank bitboxes for dense cases is not expected to be a useful optimization here.

- **Assert nonterminal marking invariants.**

    - Decision: for nonterminal roots, use debug assertions that retained nodes and retained constraints are nonempty after marking.
    - Rationale: every retained `InteriorNodeData` has a `ConstraintId`, so retained nodes imply retained constraints. An empty retained-constraint set would indicate a bug in marking.

- **Initialize overlay query storage inline.**

    - Decision: build the initial overlaid `ConstraintSetStorage` directly in `OwnedConstraintSet::query`, rather than adding a dedicated constructor.
    - Rationale: the relevant types are tightly coupled in one module, so an extra constructor would add abstraction without much benefit.

- **Keep local overlay arenas offset-free.**

    - Decision: in an overlaid query builder, local dense `nodes` and `constraints` arenas start at local index 0; `ConstraintSetBuilder` helper methods add/subtract the overlay split point when indexing storage.
    - Rationale: this avoids dummy entries and keeps allocation simple, while keeping `NodeId` and `ConstraintId` oblivious to whether their data lives in compacted overlay storage or local dense storage.

- **Keep operation caches overlay-oblivious.**

    - Decision: operation caches continue to store ordinary `NodeId`/`ConstraintId` values exactly as algorithms see them. They do not translate to local arena indexes or know whether an ID resolves to overlay or dense storage.
    - Rationale: the optional overlay is an implementation detail of `ConstraintSetBuilder` access/allocation helpers. Callers and caches should not care where an ID's data is stored.

- **Use `node_cache.is_empty()` as the lazy-population sentinel.**

    - Decision: do not add explicit booleans for whether overlay entries have been inserted into identity caches. Instead, add a small `overlay_caches_empty` helper method that documents and checks the sentinel condition, implemented in terms of `node_cache.is_empty()` when an overlay is present. Call this helper only in code paths where an overlay is present; its no-overlay behavior can be whatever keeps the implementation simplest.
    - Rationale: `OwnedConstraintSetInner` only exists for nonterminal roots, and retained nodes are nonempty by invariant, so an empty `node_cache` in an overlaid builder reliably means that overlay identity caches have not yet been populated.

- **Populate overlay identity caches before lookup/insertion.**

    - Decision: in `intern_typevar`, `intern_constraint`, `intern_interior_node`, and `typevar_id`, if an overlay exists and identity caches are empty, populate all retained overlay entries before any lookup, local allocation, or insertion. Use one shared helper for this check/populate sequence.
    - Rationale: this preserves the empty-cache sentinel invariant and ensures interning data already present in the overlay returns the old ID rather than allocating a duplicate. `typevar_id` also needs this because read-only paths can ask for typevar ordering before any interning mutation.

- **Load storage-free owned sets as terminals.**

    - Decision: if `ConstraintSetBuilder::load` receives an owned set with `inner: None`, assert that `other.node.is_terminal()` and return `ConstraintSet::from_node(self, other.node)` directly.
    - Rationale: `inner: None` is only used for terminal/storage-free owned sets, so no rebuild work is needed. If that invariant is violated, release builds should also fail at the boundary rather than proceeding with invalid storage.

- **Verify storage-free owned-set invariants in query.**

    - Decision: `OwnedConstraintSet::query` should assert that `inner: None` implies `node.is_terminal()` before constructing a no-overlay builder.
    - Rationale: this is an explicit representation invariant; violating it would make node access invalid, so all builds should fail at the boundary.

- **Derive structural traits on `OwnedConstraintSetInner`.**

    - Decision: derive the same structural traits needed by the outer owned set on `OwnedConstraintSetInner` (`Debug`, `Eq`, `Hash`, `PartialEq`, `GetSize`, `salsa::Update`, etc.) and keep derives on `OwnedConstraintSet` where possible.
    - Rationale: `Arc<T>` equality/hash are structural for `T`, so Salsa interning should continue to see content equality instead of pointer identity. Fall back to manual impls only if trait bounds cause implementation friction.

- **Prefer derives on `RankBitBox`; put manual impls there if needed.**

    - Decision: add derived traits such as `Hash` and `salsa::Update` to `RankBitBox` wherever possible. If `salsa::Update` needs a manual implementation, implement it directly on `RankBitBox`, not on `OwnedConstraintSetInner` or `OwnedConstraintSet`.
    - Rationale: keeping `RankBitBox` fully trait-compatible lets `OwnedConstraintSetInner` use straightforward derives and makes the shared utility easier to reuse.

- **Rely on derived `GetSize` for `Arc` storage.**

    - Decision: do not add custom heap-size accounting for `OwnedConstraintSet` just because it now contains `Arc<OwnedConstraintSetInner>`.
    - Rationale: existing code derives `GetSize` for types containing `Arc`, so this should be handled by the existing `get_size2` implementations.

- **Leave `reachability_constraints` unchanged.**

    - Decision: do not remove or alter the dense-case `Option<RankBitBox>` optimization in `crates/ty_python_core/src/reachability_constraints.rs`, and do not add a TODO there as part of this feature.
    - Rationale: this feature should stay focused on constraint sets.

- **Use structural equality/hash including rank bitboxes.**

    - Decision: `OwnedConstraintSet`/`OwnedConstraintSetInner` equality and hashing should include the rank bitboxes exactly as stored, including their truncated lengths.
    - Rationale: rank bitboxes are part of the ID mapping. Construction truncates deterministically to one past the highest retained ID, so structural derives are appropriate and avoid accidentally equating different mappings.

- **Trim marked `IndexVec<bool>` before building rank bitboxes.**

    - Decision: keep `RankBitBox::from_bits` unchanged and trim the marked `IndexVec<bool>` to one past the highest retained ID before passing it to `from_bits`. Use `IndexVec::iter().rposition(...)` to find the last retained bit and the existing `IndexVec::truncate(...)` method to trim. Do not add `RankBitBox::last_one()` unless a future use actually needs it.
    - Rationale: constraint-set compaction already has the marked `IndexVec<bool>` available. Trimming there avoids changing `RankBitBox::from_bits` semantics or adding unnecessary API surface.

- **Return an iterator from `RankBitBox::iter_ones()`.**

    - Decision: `RankBitBox::iter_ones()` should return `impl DoubleEndedIterator<Item = usize> + '_`, delegating to the underlying bitvec iterator while hiding the concrete bitvec type.
    - Rationale: callers can stream retained old IDs without allocation, and the public API does not expose bitvec internals.

- **Filter compacted arenas with `zip`.**

    - Decision: after trimming the marked `IndexVec<bool>`, filter each original arena by zipping the arena entries with the trimmed marks and retaining entries whose mark is `true`.
    - Rationale: `zip` naturally stops at the trimmed mark length, dropping unused high-ID tails while preserving original relative order. The compacted slice position then matches `RankBitBox::rank(old_id)`.

- **Store compacted arenas as plain boxed slices.**

    - Decision: store compacted `nodes` and `constraints` as `Box<[InteriorNodeData]>` and `Box<[Constraint<'db>]>`, not `FrozenIndexVec`.
    - Rationale: compacted positions are rank positions, not the original `NodeId`/`ConstraintId` index space, so typed index-vector storage would be misleading.

- **Do not expose direct owned-set lookup helpers.**

    - Decision: do not add public/conceptual lookup helpers that query `OwnedConstraintSet` directly. Owned sets must be read by adding them to a `ConstraintSetBuilder` via `query` or `load`.
    - Rationale: this preserves the existing owned/non-owned separation: `OwnedConstraintSet` is storage that must be interpreted in a builder context, not an independently queryable constraint set. Internal implementation code for `load` may still read `OwnedConstraintSetInner` directly because `load` is the operation that remaps owned storage into a target builder.

- **Eagerly rebuild all retained constraints in `load`.**

    - Decision: `ConstraintSetBuilder::load` should rebuild every retained compacted constraint into the target builder before recursively rebuilding nodes.
    - Rationale: this matches the current load structure, maximizes the chance of preserving ordering in the target builder, and keeps loaded-condition lookup as a compact vector indexed by `constraint_indices.rank(old_id)`.

- **Keep split arithmetic in builder methods.**

    - Decision: do not add a broad split-aware helper API on `ConstraintSetStorage`. Keep the overlay arithmetic in the existing tightly coupled `ConstraintSetBuilder` methods and small local helpers where that keeps the code simpler.
    - Rationale: `ConstraintSetBuilder` and `ConstraintSetStorage` already share responsibilities within one module; adding a larger storage abstraction would be unnecessary indirection.

- **Leave constraint typevar interning flow unchanged.**

    - Decision: do not special-case overlays in `intern_constraint_typevars`; let its existing calls to `intern_typevar` handle overlay cache population and split-aware typevar allocation.
    - Rationale: this preserves the current flow and keeps overlay behavior centralized in `intern_typevar`/shared cache-population helpers.

## Design notes

- This is a memory optimization, not a canonicalization pass. Because retained data still contains old IDs, two owned sets that differ only in historical/intermediate allocation may still compare unequal. That is acceptable for this feature.
- `OwnedConstraintSet::query` must support callbacks that create additional entries. Existing callers can do more than read: for example, solution extraction calls `remove_noninferable`, which can allocate new TDD nodes, and future callbacks may intern new constraints/typevars. Use overlaid storage so read-only callbacks can read directly from owned arenas, while mutating callbacks append new entries to small dense arenas.
- Store nonterminal `OwnedConstraintSet` storage behind a single `Arc<OwnedConstraintSetInner<'db>>` so a query builder can own one cheap clone of the compacted-storage handle instead of borrowing `OwnedConstraintSet` through an additional lifetime.
- Query-time allocation must avoid colliding with retained old IDs. When an overlay is present, newly allocated IDs for compacted `nodes` and `constraints` should start at the overlay split point (`RankBitBox::len()`, one past the highest retained old ID). Newly allocated `TypeVarId`s should start at `inner.typevars.len()`. Accessors route IDs below each split point to the overlay, and IDs at or above it to the dense arena with the split point subtracted.

## Implementation plan

### 1. Make `RankBitBox` usable in `OwnedConstraintSet`

- Import `ty_python_core::rank::RankBitBox` in `constraints.rs`.
- Add any missing traits/methods needed by `OwnedConstraintSet` derives and mapping code:
    - derive `Hash` and `salsa::Update` for `RankBitBox` if possible, because `OwnedConstraintSet` derives `Hash`/`Update` and is used by Salsa interning;
    - if `salsa::Update` cannot be derived for `RankBitBox`, implement it manually on `RankBitBox` directly so `OwnedConstraintSetInner` can still derive;
    - add a `len()` accessor and use it as the overlay split point for sparse `nodes` and `constraints`;
    - add an `iter_ones()` accessor that exposes the underlying bitvec helper for retained old-ID iteration.

### 2. Store compacted arena metadata in owned sets

- Split owned storage into an outer root plus optional inner shared storage:
    - `OwnedConstraintSet { node: NodeId, inner: Option<Arc<OwnedConstraintSetInner<'db>>> }`; keep the root on the outer value.
    - `OwnedConstraintSetInner` contains `constraints: Box<[Constraint<'db>]>`, `constraint_indices: Box<RankBitBox>`, `typevars: IndexVec<TypeVarId, BoundTypeVarIdentity<'db>>`, `nodes: Box<[InteriorNodeData]>`, and `node_indices: Box<RankBitBox>`.
    - Derive structural traits on `OwnedConstraintSetInner` so outer `OwnedConstraintSet` derives remain content-based.
- Always store a `RankBitBox` for each compacted arena (`nodes` and `constraints`), even if every entry in the retained prefix was retained. Do not preserve the reachability-constraints optimization that uses `None` for fully dense storage.
- Do not add a rank mapping for `typevars`; keep `typevars` dense/uncompacted inside the inner storage and use `inner.typevars.len()` as the typevar overlay split point.
- Update `Default` and `always()` to set `inner: None`.
- Do not add direct owned-set query helpers. Instead, implement rank-mapped storage access inside `ConstraintSetBuilder`/`ConstraintSetStorage` methods such as `interior_node_data` and `constraint_data`, so owned storage is only read after being installed as a builder overlay.

### 3. Mark and filter reachable entries in `ConstraintSetBuilder::into_owned`

- After the callback returns the extracted root `node`, mark all reachable interior nodes from that root by following `if_true`, `if_uncertain`, and `if_false`.
    - Terminal nodes need no marking.
    - Prefer an explicit stack or a small helper mirroring reachability's recursive `mark_used`; an explicit stack avoids deep recursion if large TDDs are produced.
- Mark every `ConstraintId` stored in a retained `InteriorNodeData`. Do not mark constraints from builder operation caches or other unreachable sources.
- For nonterminal roots, debug-assert that retained nodes and retained constraints are both nonempty after marking.
- Do not mark/filter `TypeVarId`s; keep `typevars` unchanged.
- For both `nodes` and `constraints`:
    - trim the marked `IndexVec<bool>` to one past the highest retained ID before constructing the `RankBitBox`;
    - build `RankBitBox::from_bits(truncated_marked_bits)` regardless of whether all entries in that prefix were retained;
    - filter the arena by zipping original arena entries with the trimmed marks and retaining marked entries;
    - store the filtered boxed slice and bitbox in `OwnedConstraintSetInner`;
    - leave old IDs inside retained data unchanged.
- If the extracted root `node` is terminal, return `OwnedConstraintSet { node, inner: None }`, even if the builder had unused storage. Otherwise, continue shrinking `typevars` before storing it uncompacted in `OwnedConstraintSetInner`.

### 4. Overlay compacted storage in query builders

`OwnedConstraintSet::query` currently clones owned arenas into `ConstraintSetStorage` and eagerly rebuilds identity caches. Replace this with overlaid storage:

- Keep `ConstraintSetStorage` as the mutable storage struct, but add an optional overlay containing a cheap clone of `Arc<OwnedConstraintSetInner<'db>>`, for example `compacted: Option<Arc<OwnedConstraintSetInner<'db>>>`. This is cloned from `OwnedConstraintSet::inner` when present.
- In a query builder, initialize the normal dense `nodes`, `constraints`, and `typevars` arenas as empty. They only hold new entries allocated during the query callback. The overlaid `inner.typevars` arena remains in `OwnedConstraintSetInner` and is read through split-aware typevar accessors.
- Accessors split IDs by the overlay split point (`RankBitBox::len()`):
    - If `compacted` is `Some` and `node.index() < compacted.node_indices.len()`, `interior_node_data` verifies that `node_indices.get_bit(node.index())` is set via an `expect`/panic path, then reads the old node from the compacted overlay through `node_indices.rank(node.index())`.
    - If the below-split bit is unset, fail loudly via that `expect`/panic path; this is an invariant violation.
    - Otherwise, subtract `node_indices.len()` and index the normal dense `nodes` arena.
    - Apply the same pattern for `ConstraintId` in `constraint_data` using `constraint_indices.len()`.
    - Apply a non-rank-mapped split for `TypeVarId`: IDs below `inner.typevars.len()` read from `inner.typevars`, and IDs at or above that length read from local `storage.typevars` after subtracting the split point.
- Allocation methods append to the normal dense arenas at local offset-free indexes but return IDs offset by the overlay split point:
    - `intern_interior_node` pushes into `storage.nodes`, receives a local `NodeId`, and returns `NodeId::new(compacted.node_indices.len() + local_id.index())` when an overlay is present.
    - `intern_constraint` pushes into `storage.constraints`, receives a local `ConstraintId`, and returns `ConstraintId::new(compacted.constraint_indices.len() + local_id.index())` when an overlay is present.
    - `intern_typevar` appends to the local dense `typevars` arena and returns `TypeVarId::new(inner.typevars.len() + local_id.index())` when an overlay is present.
- Caches must understand the split storage:
    - Add an `overlay_caches_empty` helper method for documentation and centralization; when an overlay is present, it should use `node_cache.is_empty()` as the sentinel.
    - Add one shared helper (for example `ensure_overlay_identity_caches`) used by `intern_typevar`, `intern_constraint`, `intern_interior_node`, and `typevar_id` before any lookup, local allocation, or insertion.
    - Build all three identity caches lazily as a group from overlay storage when that helper sees `overlay_caches_empty`.
    - Populate `typevar_cache` by iterating dense `inner.typevars` and inserting old `TypeVarId`s.
    - Populate `constraint_cache` by using `constraint_indices.iter_ones()` zipped with the compacted constraint slice to recover retained old `ConstraintId`s.
    - Populate `node_cache` by using `node_indices.iter_ones()` zipped with the compacted node slice to recover retained old `NodeId`s.
    - Do not allow duplicates for data already present in the overlay; interning an existing retained typevar/constraint/node must return the old ID.
- `OwnedConstraintSet::query` should construct an overlaid builder inline by cloning the single `Arc<OwnedConstraintSetInner<'db>>` when `inner` is `Some`, using empty local `nodes`/`constraints`/`typevars`. When `inner` is `None`, assert that `self.node.is_terminal()` and construct a normal empty builder with no overlay.
- Audit `constraints.rs` for direct `storage.nodes[...]`, `storage.constraints[...]`, or owned arena access and route it through `ConstraintSetBuilder`/`ConstraintSetStorage` helper methods where appropriate. This audit is critical because local overlaid arenas use offset-free indexes while `NodeId`/`ConstraintId` values include the overlay split point. `ConstraintSetBuilder::load` remains an eager remapping operation and may read `OwnedConstraintSetInner` directly with rank-mapped invariant checks.

### 5. Preserve load behavior

- If `other.inner` is `None`, assert that `other.node.is_terminal()` and return `ConstraintSet::from_node(self, other.node)` directly.
- Otherwise, keep `load` as an eager remapping operation. Because the target builder can have different typevar and constraint ordering, `load` cannot reuse owned IDs directly; it should eagerly rebuild all retained compacted constraints into the target builder, then rebuild the retained node graph into target-builder nodes.
- When loading retained constraints into the target builder, build a **load-local** compact vector aligned with the retained compacted constraint slice. Its entries are target-builder `NodeId`s for the reloaded conditions, so this vector cannot live in `OwnedConstraintSetInner`.
    - To look up the loaded condition for an old `ConstraintId`, verify that the old ID is below `constraint_indices.len()` and that its bit is set, then use `constraint_indices.rank(old_id.index())` to index the load-local compact loaded-condition vector.
    - This avoids building a sparse `IndexVec<Option<NodeId>>` or `FxHashMap`, and avoids adding `select` to `RankBitBox` for this feature.
- When recursively rebuilding old nodes, read retained node data from `OwnedConstraintSetInner` using `node_indices.rank(old_node.index())` after verifying the bit is set. The target builder created by `load` remains dense and does not inherit the owned set's overlay.

### 6. Tests

Add unit tests in the existing `#[cfg(test)] mod tests` in `constraints.rs`.

Recommended cases:

1. **Compaction drops unreachable nodes and constraints**
    - In `into_owned`, create unused constraints/nodes first, then create and return a later set.
    - Assert that the owned set has `inner: Some`, fewer nodes and constraints than the builder allocated, and rank bitboxes whose lengths are the expected overlay split points.
    - Assert that `typevars` remains dense/uncompacted in the inner storage.
    - Make at least one returned old ID larger than the compacted storage length to prove rank mapping is required.
1. **Read-only `query` reads from the overlay**
    - Query the compacted set, check its display or satisfiability, and assert (inside the test module) that the builder has a compacted overlay and empty local new-node/new-constraint/new-typevar arenas.
1. **Mutating `query` allocates after the overlay**
    - Query the compacted set, perform operations that allocate nodes and, if practical, new constraints/typevars, assert expected semantics, and assert that newly allocated IDs are greater than or equal to the overlay split points.
1. **`load` works on compacted owned sets**
    - Load the compacted owned set into a fresh builder and verify display/satisfiability matches the original returned set.
1. **Terminal root discards all storage**
    - Optionally build some unused entries but return `always` or `never`; assert `inner` is `None` and querying/loading still works.

Suggested test command:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic constraints::
```

If `cargo nextest` is unavailable, use the corresponding `cargo test -p ty_python_semantic constraints::` fallback.

### 7. Final checks

- Run the targeted tests above.
- If any snapshots are created or updated, review them before finishing.
- Run prek for changed files via `/home/dcreager/bin/jpk run --files <changed paths>`.
- Review the final diff with `jj diff --git`.
