# PR 2A: scoped quantified-relation storage

## Status

- [x] PR 0's C0/E1–E6 behavior basis already exists on the parent stack.
- [x] PR 1B's visitor-driven `PathAssignments` implementation is already merged into `main`.
- [x] Phase 2 — Refactor `TypeVarSet` to retain identity-keyed bound instances.
- [ ] Phase 3 — Integrate existential atoms, ownership, mapping, and display end to end.

The former Phase 1 `Atom::Range` refactor ([astral-sh/ruff#27111](https://github.com/astral-sh/ruff/pull/27111), `dcreager/refactor-atoms`) is **superseded and is not a prerequisite**. It has been moved out of this stack. `dcreager/inferable-instances` is based directly on `dcreager/quant-tests`; the implementation therefore still uses `ConstraintId`, `Constraint`, and the existing range-constraint arenas. Do not reintroduce the single shared `Atom` payload arena described by that earlier design.

This document is the ground truth for phase ordering and dependencies. Phases should normally be completed in the order listed. An agent resuming this plan must read the relevant repository files and verify that the status markers accurately reflect the implementation before continuing.

Each implementation phase is a self-contained unit of work in its own jj revision. Before changing files for a phase, create a new revision with `jj new -A @`; never use `jj edit`, and describe the revision with a `[π]`-prefixed `jj describe` message. Update this plan's status in the revision completing each phase. Documentation and tests are cross-cutting responsibilities of every phase, not separate phases. The full relevant test suite must pass at the end of every phase. Never comment out or ignore a temporarily failing test: update its assertion or mdtest expectation to the current behavior and add a clear TODO describing the eventual correct behavior.

## Scope and revised design

The original design document proposes both a companion domain on every `ConstraintSet` and a stored domain inside each quantified-relation atom:

```rust
struct ConstraintSet {
    domain: NodeId,
    relation: NodeId,
}

struct Existential<'db> {
    locals: TypeVarSet<'db>,
    domain: NodeId,
    body: NodeId,
}
```

For this implementation, omit **only** the companion domain on `ConstraintSet`. PR 1A is not a prerequisite, and `ConstraintSet` retains its existing single root. A quantified relation does need its own domain because latent restrictions, such as receiver-binding constraints, belong to the quantifier's domain rather than its body. Its binder is the existing compact, Salsa-interned `TypeVarSet` representation:

```rust
struct Existential<'db> {
    locals: TypeVarSet<'db>,
    domain: NodeId,
    body: NodeId,
}
```

Construction takes an explicitly supplied `additional_domain` constraint set and combines it with the declared valid specializations of precisely the quantified variables:

```text
DeclaredDomain(X) = ∧ { valid_specializations(x) | x ∈ X }
Domain(X, Additional) = DeclaredDomain(X) ∧ Additional

positive relation:  ∃X. Domain(X, Additional) ∧ Body(X, Y)
negative relation:  ¬∃X. Domain(X, Additional) ∧ Body(X, Y)
```

`X` is exactly the explicitly supplied `TypeVarSet`. If `x`'s declared bound mentions an outer variable `y`, the generated domain can contain the relationship `x ≤ y`, but **must not** recursively include `valid_specializations(y)` unless `y` was explicitly included in `X`. Outer/free variables mentioned by declared bounds or `additional_domain` remain free and belong to the enclosing scope.

Latent constraints, such as receiver-binding constraints, are supplied as `additional_domain`. Keeping them in the stored quantifier domain is essential for universal quantification: a receiver restriction must be part of the implication's antecedent, not accidentally moved into its consequent. No domain is added to `ConstraintSet`.

One existential atom kind is sufficient. A universal relation is represented later by a negative existential atom whose body is negated:

```text
∀X. Domain(X, Additional) ⇒ Body(X, Y)
    ≡ ¬∃X. Domain(X, Additional) ∧ ¬Body(X, Y)
```

PR 2A provides representation, persistence, and an explicit test-only eager-lowering operation. Positive/nested scoped discharge, negative discharge and explicit incompleteness, witness-preserving solution selection, optional exact-cover optimizations, and generic-signature integration remain subsequent PRs. Do not change production `reduce_inferable` or `for_all` to introduce existential atoms, and do not automatically lower constraint sets during production interrogation. Encountering an existential in a legacy semantic consumer is an invariant violation and may panic. A separately scoped follow-up PR can decide whether to start constructing existential atoms and explicitly opt into lowering before full scoped traversal lands.

## Current implementation and constraints

### Existing baseline

- `crates/ty_python_semantic/resources/mdtest/type_properties/quantification.md` already contains the C0/E1–E6 matrix, including current-result assertions and TODOs for behavior that later quantifier PRs must fix.
- `crates/ty_python_semantic/src/types/call/bind.rs` exposes `ConstraintSet.exists(tuple[...])` and `ConstraintSet.for_all(tuple[...])` to mdtests, but currently routes them to eager `reduce_inferable` and `for_all`.
- `crates/ty_python_semantic/src/types/constraints.rs` contains the existing visitor-driven `PathAssignments`, `PathVisitor`, and `PathFold` from PR 1B. Existing ordinary-range behavior must not regress.
- The current implementation has no `Atom`, `AtomId`, `Existential`, or `ExistentialId`. `InteriorNodeData::constraint` is a `ConstraintId` and both builder and owned storage retain their existing range-constraint arenas.

### Bound instances and declared-domain construction

Before Phase 2, the type-variable set in `crates/ty_python_semantic/src/types/generics.rs` contained only `BoundTypeVarIdentity`s. An identity deliberately excludes the declared bound/constraints, so it cannot provide the instance needed by the existing:

```rust
BoundTypeVarInstance::valid_specializations(db, builder) -> NodeId
```

Phase 2 moves the renamed `TypeVarSet` definition into `crates/ty_python_semantic/src/types/typevar.rs` and changes the existing Salsa-interned set representation to retain bound instances while still using their identity for membership and deduplication:

```rust
#[salsa::interned]
struct TypeVarSetInner<'db> {
    typevars: FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
}
```

This matches the existing representation of `GenericContext`. Instances with the same identity can be assumed to have equivalent bounds; differences can reflect whether bounds have been eagerly evaluated. Construction and merging deduplicate by identity with deterministic **first-wins** semantics. Lookup remains keyed by identity, rather than using an `FxOrderSet<BoundTypeVarInstance>` whose equality would incorrectly distinguish different instances of the same logical variable.

With instances available directly in `TypeVarSet`, a quantified variable need not already occur in the body, `additional_domain`, or a builder-local arena before its declared domain is constructed. No additional canonical-instance storage or recovery mechanism is needed in `ConstraintSetStorage` or `OwnedConstraintSetInner`. Preserve fresh occurrence identities, `ParamSpec` attribute distinctions, Salsa sharing, insertion ordering, and existing identity-based membership.

### Existing range-only assumptions

`ConstraintId` currently indexes the arena of ordinary range constraints:

```rust
#[newtype_index]
struct ConstraintId;

struct Constraint<'db> {
    typevar: BoundTypeVarInstance<'db>,
    bounds: ConstraintBounds<'db>,
}
```

Many paths assume every TDD atom is a range constraint and has `.typevar` and `.bounds`, including:

- `SequentMap` generation and implication/disjointness checks;
- `PathAssignments` discovery and derived assignments;
- `PathBounds::compute` and its simple-conjunction fast path;
- eager existential abstraction and `remove_noninferable`;
- `satisfied_by_all_typevars` and constraint-support discovery;
- simplification and DNF/graph display;
- `apply_type_mapping_impl`;
- `ConstraintSetBuilder::into_owned`, `OwnedConstraintSet::query`, and `ConstraintSetBuilder::load`; and
- `OwnedConstraintSet::types`, used by receiver-constraint mapping in `signatures.rs`.

A quantified relation is an opaque Boolean atom at its containing TDD level. Structural TDD operations and source ordering must treat it as a first-class atom. Range-only reasoning must continue to operate on `ConstraintId`/`Constraint` and must never inspect an existential as if it had range bounds. Unsupported legacy semantic consumers should fail with an explicit invariant violation if they encounter an existential.

### Atom-versus-constraint terminology

Keep **constraint** for the existing range representation and all genuinely range-specific operations. In particular, retain:

- `ConstraintId`, `Constraint`, and `ConstraintBounds`;
- `ConstraintSetStorage::constraints`, `OwnedConstraintSetInner::constraints`, `constraint_cache`, and `constraint_indices`;
- `retained_constraint_index`, `adjusted_constraint_id`, `intern_constraint`, `constraint_data`, and other range lookup/interning helpers;
- range implication/intersection, bound-depth caches, `ConstraintAssignment`, and range-only sequent handling.

Use **atom** only for data structures and operations shared by range and existential node atoms. The discriminator is the new `AtomId` enum, and the generic interior-node field becomes `atom: AtomId`. Rename traversal callbacks, root lookup, structural temporary maps, and similar generic node operations from `constraint` to `atom` only where they can now receive either kind. Do not rename the existing range arena or wrap `Constraint` in a single-variant payload enum.

The existential representation has its own typed arena, index space, cache, and compacted-index metadata. This keeps the common range representation and its hot paths intact while making it impossible to confuse range and existential payload IDs.

## Proposed representation

Keep the existing range IDs and payloads unchanged. Add a separate existential ID/payload arena and make the ID stored in an interior node the discriminator:

```rust
#[newtype_index]
struct ConstraintId;

#[newtype_index]
struct ExistentialId;

enum AtomId {
    Range(ConstraintId),
    Existential(ExistentialId),
}

struct Constraint<'db> {
    typevar: BoundTypeVarInstance<'db>,
    bounds: ConstraintBounds<'db>,
}

struct Existential<'db> {
    locals: TypeVarSet<'db>,
    domain: NodeId,
    body: NodeId,
}

struct InteriorNodeData {
    atom: AtomId,
    if_true: NodeId,
    if_uncertain: NodeId,
    if_false: NodeId,
    source_order: usize,
    max_source_order: usize,
}
```

Builder and owned storage therefore have separate arenas and interning/index metadata for the two payload kinds:

```rust
struct ConstraintSetStorage<'db> {
    constraints: IndexVec<ConstraintId, Constraint<'db>>,
    existentials: IndexVec<ExistentialId, Existential<'db>>,
    // ...
    constraint_cache: FxHashMap<Constraint<'db>, ConstraintId>,
    existential_cache: FxHashMap<Existential<'db>, ExistentialId>,
}

struct OwnedConstraintSetInner<'db> {
    constraints: Box<[Constraint<'db>]>,
    constraint_indices: RankBitBox,
    existentials: Box<[Existential<'db>]>,
    existential_indices: RankBitBox,
    // ...
}
```

The exact visibility and helper names can follow neighboring code. The essential properties are:

1. Existing range constraints remain cheap, copyable, and fast. `ConstraintId` continues to index only `Constraint` payloads; range-specific APIs, caches, and assignments remain strongly typed.
1. `ExistentialId` indexes only `Existential` payloads in a separate builder arena and separate compacted owned arena. Existentials have their own interning cache and overlay split/index metadata; there is no shared `Atom` payload arena or shared payload cache.
1. `AtomId::{Range, Existential}` is the only ID stored in an interior node and is used by generic TDD structure, traversal, and Boolean operations. Exhaustive matching selects the correctly typed payload arena.
1. `AtomId` must provide a deterministic total TDD-variable ordering across both ID spaces while preserving the existing range ordering/wobbling behavior. Raw per-arena indices cannot be treated as one shared encounter sequence. Explicitly define the cross-kind ordering (or retain separate builder-local encounter-order metadata if interleaving is required), and preserve source-order offsets independently of that ordering.
1. `TypeVarSet` is already a compact, copyable, Salsa-interned set, so each `Existential` can store its binder directly. No boxed local slice or additional canonical-instance storage is needed.
1. Locals are exactly the supplied `TypeVarSet`, which retains bound instances while preserving identity-based membership, first-wins deduplication, freshness distinctions, Salsa sharing, and efficient set representation.
1. The construction boundary accepts the authoritative `TypeVarSet`, an `additional_domain` constraint set, and a body. It stores that set unchanged and adds no variables discovered recursively in bounds.
1. The relation stores a domain root equal to the conjunction of the selected variables' valid specializations and `additional_domain`. `ConstraintSet` itself never stores a companion domain.
1. The relation stores **no free-variable interface**; compute it from the stored domain and body when required. Ephemeral builder-local support caches are permitted if measurements justify them.
1. Nested quantified-relation domains and bodies remain separate TDDs. A relation's local variables do not become ordinary variables in its containing TDD merely because their graphs share the same builder arenas.

A relation's free interface is the union of the free support of its stored domain and body, minus its own local set. This includes free variables introduced by both declared bounds and `additional_domain`. For nested quantified atoms, use the nested atom's free interface rather than exposing its locals. In particular:

```text
locals = {X}
X has declared upper bound Y
body mentions X but not Y
free interface = {Y}
```

`Y` is an interface variable, not an additional quantified variable, and its own declared domain is not included in `DeclaredDomain({X})`. An `additional_domain` may explicitly impose restrictions involving `Y`; those restrictions remain inside this quantifier's stored domain without quantifying `Y`.

## Former Phase 1 — superseded `Atom::Range` refactor

**Status:** superseded; not part of the implementation stack.

**Former PR:** [astral-sh/ruff#27111](https://github.com/astral-sh/ruff/pull/27111) (`dcreager/refactor-atoms`).

The earlier phase replaced the existing constraint arena with a shared arena containing a single-variant `Atom::Range(Constraint)` enum and renamed `ConstraintId` and the associated generic/range storage helpers. That representation is no longer desired. The branch has been moved out of the way, and Phase 2 has been rebased directly onto PR 0. Phase 3 must start from the existing `ConstraintId`/`Constraint` representation and add an independent existential arena plus the `AtomId` discriminator instead.

## Phase 2 — Refactor `TypeVarSet` to retain identity-keyed bound instances

**Status:** complete.

**PR:** [astral-sh/ruff#27113](https://github.com/astral-sh/ruff/pull/27113) (`dcreager/inferable-instances`).

**Dependency:** existing PR 0 and merged PR 1B only. This is a behavior-preserving prerequisite and is independent of the superseded Phase 1 refactor.

1. Change `TypeVarSetInner` from an ordered set of `BoundTypeVarIdentity`s to an ordered map from identity to `BoundTypeVarInstance`, following `GenericContext::variables_inner`.
1. Update the generic-context collector and mdtest tuple extraction to retain the instances they already encounter. Update the existing constraint-unit-test constructors to pass instances rather than first discarding them.
1. Preserve identity-based membership for both `BoundTypeVarIdentity::is_inferable` and `BoundTypeVarInstance::is_inferable`, including the caller that explicitly normalizes `ParamSpec` attributes before checking membership.
1. Update the existing `TypeVarSet::merge` implementation to combine maps with stable insertion order and identity-keyed first-wins semantics. Test equivalent eager/lazy instances with the same identity, distinct freshness nonces, and distinct `P.args`/`P.kwargs` identities.
1. Update existing iteration and debug-display behavior as needed, without forcing evaluation of lazily represented bounds merely to construct, merge, or inspect the set. Do not add declared-domain helpers, existential constructors, new type-variable-set methods, or any other functionality required only by later phases.
1. Audit Salsa interning, tracked `merge`, existing cache keys such as `exists_cache`, identity-based ordering, and memory usage so equivalent instance representations do not introduce avoidable cache churn or behavior changes.
1. Add focused regression coverage for empty sets, identity-keyed first-wins construction and merging, eager/lazy equivalent instances, fresh identities, `ParamSpec` components, and preservation of lazy bounds. Do not add quantifier-domain or existential-specific tests in this refactoring revision.
1. Run the existing quantification and ordering mdtests without changing their expected behavior. Keep `ConstraintId`, `Constraint`, `ConstraintSetStorage`, `OwnedConstraintSetInner`, and existing existential-abstraction algorithms unchanged.

**Exit criteria:** `TypeVarSet` retains actual bound instances while preserving its existing identity-based membership, ordering, first-wins deduplication, laziness, Salsa/cache behavior, and observable inference results; no new existential-specific methods or domain computations have been introduced; the revision is independently mergeable as a pure refactor.

**Validation note:** the focused `TypeVarSet` and generic-context unit tests, quantification and constraint-ordering mdtests, and full `ty_python_semantic` suite pass (769 passed, 35 skipped). No snapshot expectations changed.

## Phase 3 — Integrate existential atoms, ownership, mapping, and display end to end

**Status:** pending.

**Dependency:** the independently reviewable Phase 2 `TypeVarSet` representation refactor, existing PR 0, and merged PR 1B. The superseded `Atom::Range` refactor is not a dependency.

Adding `AtomId::{Range, Existential}`, changing `InteriorNodeData::constraint` to `atom: AtomId`, and adding the separate existential arenas immediately affects exhaustive matches, `ConstraintSet`, `OwnedConstraintSet`, type mapping, and display. Treat all of the workstreams below as one self-contained implementation phase and jj revision, not as independently passing revisions. If implementation reveals a genuinely self-contained intermediate boundary, update this plan before splitting the phase.

### Declared domains, existential atoms, and free interfaces

1. Add a `TypeVarSet`-level helper that computes the conjunction of `BoundTypeVarInstance::valid_specializations` for exactly its stored instances in the supplied builder. Reuse the existing single-typevar implementation for unbounded variables, upper bounds, finite constraints, gradual materialization, and `ParamSpec` components; do not recursively add domains for variables mentioned in those bounds.
1. Support binders whose variables do not otherwise occur in the body, `additional_domain`, or builder. Their instances are available directly from `TypeVarSet`; do not introduce canonical-instance recovery in builder or owned storage.
1. Preserve deterministic source ordering when constructing declared-domain roots. Never add a companion domain field to `ConstraintSet` or a separate top-level domain field to `OwnedConstraintSet`.
1. Add a copyable `Existential<'db>` payload containing the supplied Salsa-interned `TypeVarSet<'db>`, a stored domain `NodeId`, and a separate body `NodeId`; add its typed `ExistentialId`, builder arena, and interning cache. Keep the existing `ConstraintId`, `Constraint`, constraint arena, and constraint cache unchanged.
1. Introduce `AtomId::Range(ConstraintId)` and `AtomId::Existential(ExistentialId)`, and change the generic interior-node field to `atom: AtomId`. Update generic TDD construction, root lookup, traversal callbacks, temporary maps, ordering, and Boolean operations to use `AtomId`; retain `ConstraintAssignment` and sequent/bound caches as range-only types.
1. Update existing range-specific consumers to match `AtomId::Range` before using `ConstraintId` or loading `Constraint` data. Handle existential atoms in structural operations, and panic with a clear invariant message if one unexpectedly reaches legacy satisfiability, validity, solution extraction, eager abstraction, or other unsupported semantic consumers.
1. Add a private/internal constructor that takes an explicitly supplied `TypeVarSet`, an `additional_domain: ConstraintSet`, and a body; verifies all constraint-set inputs use the same builder; retains the supplied type-variable set unchanged; computes `valid_specializations(locals) ∧ additional_domain`; interns the `Existential` in its own arena; and creates an interior node using `AtomId::Existential`.
1. Preserve the correct empty-binder fast path: with no locals, the quantified relation is `additional_domain ∧ body`, not merely `body`. Avoid unsupported simplifications that would accidentally assume a nonempty binder's domain is inhabited or flatten a nested scope.
1. Enforce binder invariants with debug assertions: stored bound instances match their identity keys, nested binder identities are fresh/disjoint where required, and scope is interpreted according to lexical nesting rather than TDD variable ordering.
1. Compute free support recursively from both the stored domain and body. Remove only the current binder's locals; preserve outer variables referenced through declared bounds or `additional_domain`, and preserve only the free interface of nested relation atoms.
1. Ensure support discovery walks a TDD as a DAG and does not recurse into a variable's bound merely to decide which variables are quantified. Cache computed support only if necessary and only in transient builder storage.
1. Give quantified atoms stable ordinary TDD ordering and Boolean composition. Define a deterministic total order across range and existential IDs, preserve existing range wobbling, and preserve source-order offsets when combining declared valid-specialization roots with `additional_domain`. Test positive, negative, and uncertain outer edges structurally, including nested relation domains and bodies.
1. Keep existential constructors private/internal. Existing `ConstraintSet.exists`, `ConstraintSet.for_all`, `reduce_inferable`, and signature checking must continue to construct and use their current eager production representation; do not switch them to creating existential atoms in this PR.
1. Add declared-domain and structural tests for empty binders, unconstrained variables, upper-bounded variables, finite constrained variables, gradual bounds, `ParamSpec` attributes, unused binders, interning/deduplication, distinct binders/domains/bodies, separate ID arenas, nested scopes, shared domain/body DAGs, receiver-style additional equality constraints, contradictory additional domains, additional domains mentioning only outer variables, dependent declared bounds, and TDD ordering wobble.

### Explicit test-only lowering and production invariants

1. Add an explicit lowering operation for existential atoms constructed by Rust unit tests. Recursively lower nested domains and bodies, then replace each existential with the existing eager abstraction of `domain ∧ body` over its `locals`. If practical, compile this helper only for tests until a later production user exists.
1. Rebuild surrounding TDD structure while preserving positive/negative polarity, uncertain-branch semantics, source ordering, atom ordering, and shared subgraphs. Storage, Boolean structure, ownership, type mapping, and display must retain existential atoms unless a test explicitly requests lowering.
1. Never call lowering automatically from production satisfiability/validity checks, `satisfied_by_all_typevars`, solution extraction, `reduce_inferable`, `for_all`, or other semantic interrogation. Production does not construct existential atoms, so existing hot paths must pay no no-op scan, cache lookup, or normalization cost.
1. Panic with an explicit invariant-violation message if an existential nevertheless reaches a legacy semantic consumer. Add focused tests documenting the panic and tests that explicitly lower first before interrogating the equivalent ordinary constraint set.
1. Treat explicit test lowering as the existing eager algorithm with its existing limitations, not as correct scoped quantifier discharge. Do not interpret existential atoms as independent freely assignable Booleans or silently claim exact invariant projection.
1. Add unit tests comparing explicitly lowered positive, negative, nested, and uncertain existential formulas against manually constructing `domain ∧ body` and invoking the current eager abstraction. Include finite declared domains, receiver-style additional domains, free outer variables, and known incomplete invariant cases.
1. Leave switching production `reduce_inferable` to construct existential atoms as an optional, separately reviewable follow-up. That future PR may explicitly invoke lowering where required and must independently evaluate declared-domain behavior changes and performance.

### Owned storage, compaction, overlays, and loading

1. Extend compacted owned storage with a separate `existentials: Box<[Existential]>` arena and `existential_indices` rank/index metadata alongside the unchanged `constraints` and `constraint_indices`. Retain the existing terminal fast path and `Arc` sharing. Do not introduce a shared `Atom` payload arena.
1. Update `ConstraintSetBuilder::into_owned` reachability traversal to match each interior `AtomId`: mark range IDs in the existing constraint bitset; mark existential IDs in a separate existential bitset; traverse each retained existential's stored-domain and body nodes/atoms; and include all nested quantified relations. Bound instances remain directly available from each relation's Salsa-interned `TypeVarSet`.
1. Persist each retained relation's existing Salsa-interned `TypeVarSet`, stored domain, and body in the compacted existential arena, but never persist transient support caches or add a companion domain to `OwnedConstraintSet`. Preserve sparse retained IDs independently for both arenas.
1. Extend compacted-overlay existential access, retained-index lookup, identity-cache initialization, and existential interning so `OwnedConstraintSet::query` can read both typed arenas and append new range or existential payloads after the correct per-kind overlay split. Existing constraint overlay behavior must remain unchanged.
1. Update `ConstraintSetBuilder::load` to rebuild nested domains and bodies before interning their containing existential, map each `AtomId` through the correct arena, reuse each relation's globally valid `TypeVarSet` unchanged, preserve source-order offsets, and preserve sharing for repeated relation/domain/body DAGs. Do not assume `ConstraintId` and `ExistentialId` share an index space or encounter order.
1. Audit `OwnedConstraintSet::types`: it must expose range types, types in quantified domains and bodies, bound instances retained directly by `TypeVarSet`, and free interfaces introduced by declared bounds or `additional_domain`. If inspecting declarations requires `db`, update the small set of callers rather than losing mappings of relevant outer variables.
1. Add owned-storage tests for unreachable relation/domain/body compaction, nested scopes, shared domain/body subgraphs, sparse retained IDs in both arenas, read-only overlay queries, mutation after overlay, cross-builder remapping, receiver-style additional domains, dependent declared bounds, fresh locals, and the storage-free terminal fast path.

### Type mapping, display, lexical scope, and deterministic ordering

1. Extend `ConstraintSet::apply_type_mapping_impl` to match the interior `AtomId`, retain its existing range mapping behavior, recursively rebuild both quantified domains and bodies, and, when a mapping actually renames binder variables, construct the corresponding mapped `TypeVarSet` and re-intern the mapped existential in its own arena.
1. Specify capture-avoiding binder behavior explicitly. Free/interface variables must be mapped; deliberate freshness/typevar-to-typevar renaming must update binder, domain, and body together; mappings that would replace a bound local with a concrete type must have one documented, tested policy rather than silently capturing or freeing that variable.
1. Ensure declared bounds of locals and additional-domain restrictions track mapped/freshened bound instances, including outer/free variables appearing only in the stored domain.
1. Extend concise constraint display and full graph display to match the interior `AtomId` and show existential binders, their separate domains and bodies, negated polarity, uncertain branches, source order, and shared graphs. Do not introduce a separate universal atom or suggest that the outer `ConstraintSet` itself owns a domain.
1. Update range-clause implication/simplification so it continues to use `ConstraintId` range containment/disjointness only for range literals. Quantified literals compare opaquely and are never simplified using range operations.
1. Preserve deterministic local order, total TDD atom order, graph numbering, and source order across normal, reverse, and XOR builder orderings, cross-builder load, interning in separate arenas, and type mapping.
1. Add targeted unit tests for free-variable specialization, freshness renaming, nested binders, capture avoidance, local declared-bound mapping, mapped receiver-style additional domains, positive/negative/uncertain display, domain/body graph sharing, and ordering stability. Keep the PR 0 mdtest expectations unchanged unless a separately implemented behavior genuinely improves; annotate any changed intermediate expectations with clear TODOs.
1. Recheck the existing receiver-constraint `OwnedConstraintSet::types`/`Signature::map_constraints` integration and avoid exposing residual atoms through the mdtest `exists`/`for_all` entry points prematurely.

**Exit criteria:** existing range IDs, payloads, arenas, caches, and hot paths remain strongly typed and intact; existential atoms retain their explicit binders, combined domains, and separate bodies in their own arenas; `AtomId` safely discriminates the two payload ID spaces in every interior node and has deterministic total ordering; existential atoms survive owned compaction, overlay queries, cross-builder loading, type mapping, and display; unit tests can explicitly lower them through the existing eager abstraction; unsupported production interrogation panics instead of silently guessing or performing a no-op normalization; production `reduce_inferable` and `for_all` do not create existential atoms; `ConstraintSet` has no companion domain; existing flat-TDD behavior, performance-sensitive fast paths, and all tests remain intact.

## Validation for every implementation phase

Run the focused baseline first:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
CARGO_PROFILE_DEV_DEBUG="line-tables-only" \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic -- mdtest::type_properties/quantification.md
```

Run the affected constraint-ordering mdtest when atom ordering, source order, loading, or support handling changes:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
CARGO_PROFILE_DEV_DEBUG="line-tables-only" \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic -- mdtest::regression/constraint_set_ordering.md
```

Finish every phase with the full relevant crate suite:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 \
INSTA_FORCE_PASS=1 \
INSTA_UPDATE=always \
CARGO_PROFILE_DEV_DEBUG="line-tables-only" \
MDTEST_UPDATE_SNAPSHOTS=1 \
cargo nextest run -p ty_python_semantic
```

If `cargo nextest` is unavailable, use the documented `cargo test` equivalents. Never run concurrent cargo commands in this workspace. Review all generated snapshot changes and check for `.pending-snap` files when applicable; never edit snapshot contents manually.

For a jj worktree, run the repository hooks through the installed wrapper and pass every file changed in the current phase:

```sh
/home/dcreager/bin/jpk run --files <changed-file> ...
```

## Explicit non-goals for PR 2A

- Adding `ConstraintSet { domain, relation }` or reviving PR 1A.
- Reintroducing the superseded `Atom::Range(Constraint)` payload enum, replacing the existing constraint arena/cache, or combining range and existential payloads into one shared arena/index space.
- Storing a companion domain on `ConstraintSet`, a globally deferred quantifier set, a boxed/builder-local binder set, or a cached free-interface field; storing the supplied `TypeVarSet` and combined domain on `Existential` in its own arena is required.
- Recursively adding declared domains for variables mentioned in a quantified variable's bound.
- Adding a `UniversalRelation` atom kind.
- Switching production `reduce_inferable`, `for_all`, signature comparison, or mdtest quantifier methods to construct residual existential atoms in this PR; a later, smaller PR may opt into that behavior and explicitly invoke lowering where needed.
- Automatically scanning for or lowering existential atoms on production semantic-query hot paths.
- Positive/nested path discharge, witness-family extraction, negative evaluation, incompleteness reporting, exact-cover optimization, receiver-specific quantifier partitions, or recursive-protocol feature work.
- Solving every C0/E1–E6 TODO in this storage-only PR; those tests are the contract for the later stack.
