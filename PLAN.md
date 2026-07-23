# PR 2A: scoped quantified-relation storage

## Status

- [x] PR 0's C0/E1–E6 behavior basis already exists on the parent stack.
- [x] PR 1B's visitor-driven `PathAssignments` implementation is already merged into `main`.
- [x] Phase 2 — Refactor `TypeVarSet` to retain identity-keyed bound instances.
- [ ] Phase 3 — Implement the minimal existential-atom representation, ownership, and display.

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

Construction takes an explicitly supplied `additional_domain` TDD root and combines it with the declared valid specializations of precisely the quantified variables:

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

PR 2A is a representation prerequisite, not a production quantification feature. Its agreed minimum is the existential-atom representation together with ownership and display. Other capabilities are not requirements merely because they might be useful to a future consumer: include them only when a concrete representation, ownership, or display acceptance case actually requires them. Ownership must support both `OwnedConstraintSet::query` and `ConstraintSetBuilder::load`, including adding new existential atoms after existing ones have been loaded. Both `ConstraintSet::display` and `ConstraintSet::display_graph` must support existential atoms. Positive/nested scoped discharge, negative discharge and explicit incompleteness, witness-preserving solution selection, optional exact-cover optimizations, and generic-signature integration remain subsequent PRs. Do not change production `reduce_inferable` or `for_all` to introduce existential atoms, and do not automatically lower constraint sets during production interrogation.

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
- range implication/intersection, bound-depth caches, and range-only sequent handling.

Use **atom** only for data structures and operations shared by range and existential node atoms. The discriminator is the new `AtomId` enum, the generic interior-node field becomes `atom: AtomId`, and the existing `ConstraintAssignment` becomes `AtomAssignment` over `AtomId`. Rename traversal callbacks, root lookup, structural temporary maps, and similar generic node operations from `constraint` to `atom` only where they can now receive either kind. Range-specific consumers of an `AtomAssignment` must explicitly require a range atom rather than inspecting an existential as a range constraint. Do not rename the existing range arena or wrap `Constraint` in a single-variant payload enum.

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

enum AtomAssignment {
    Positive(AtomId),
    Negative(AtomId),
    Unconstrained(AtomId),
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

1. Existing range constraints remain cheap, copyable, and fast. `ConstraintId` continues to index only `Constraint` payloads; range-specific APIs, caches, and sequent reasoning remain strongly typed, while generic atom assignments explicitly distinguish their underlying atom kind.
1. `ExistentialId` indexes only `Existential` payloads in a separate builder arena and separate compacted owned arena. Existentials have their own interning cache and overlay split/index metadata; there is no shared `Atom` payload arena or shared payload cache.
1. `AtomId::{Range, Existential}` is the only ID stored in an interior node and is used by generic TDD structure, traversal, and Boolean operations. Exhaustive matching selects the correctly typed payload arena.
1. `AtomId` must provide a deterministic total TDD-variable ordering across both ID spaces while preserving the existing range ordering/wobbling behavior. Order first by atom kind, with ordinary range constraints closer to the TDD root under the normal ordering, and then by the existing reversed/wobbled per-kind ID ordering. Apply `wobble_index` to the atom kind as well so wobble runs also exercise the opposite cross-kind ordering. Raw per-arena indices do not form a shared encounter sequence, and no global encounter-order metadata is needed. Preserve source-order offsets independently of TDD ordering.
1. `TypeVarSet` is already a compact, copyable, Salsa-interned set, so each `Existential` can store its binder directly. No boxed local slice or additional canonical-instance storage is needed.
1. Proactively intern every variable in the supplied binder into the builder's existing typevar arena before constructing any declared-domain constraints, preserving binder/source order even for unused or unconstrained variables. Binder interning itself is just an ordered iteration of the explicit instances calling existing `intern_typevar`; do not recursively inspect bounds, constraints, or defaults. Leave existing range-constraint interning and its incidental traversal of referenced typevars completely unchanged.
1. Locals are exactly the supplied `TypeVarSet`, which retains bound instances while preserving identity-based membership, first-wins deduplication, freshness distinctions, Salsa sharing, and efficient set representation. Constructors trust the caller to provide correctly scoped/fresh binder identities; document this invariant, but do not traverse nested graphs or enforce freshness/disjointness with debug assertions.
1. The internal construction boundary accepts the authoritative `TypeVarSet` plus `NodeId` roots for `additional_domain` and the body. It stores the binder unchanged and adds no recursively discovered variables to that binder.
1. The relation stores a domain root equal to the conjunction of the selected variables' valid specializations and `additional_domain`. `ConstraintSet` itself never stores a companion domain.
1. The relation stores **no free-variable interface** and introduces no free-support traversal, helper, or cache.
1. Nested quantified-relation domains and bodies remain separate TDDs. A relation's local variables do not become ordinary variables in its containing TDD merely because their graphs share the same builder arenas.

If a future operation ever needs to reason explicitly about free variables, the semantic free variables of a relation would come from its stored domain and body minus its own binder. This is explanatory semantics, not a planned data structure, traversal, helper, or cache; Phase 3 must not implement free-interface/support discovery, and no later need is assumed. In particular:

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

**Validation note:** the focused `TypeVarSet` and generic-context unit tests, quantification and constraint-ordering mdtests, and full `ty_python_semantic` suite pass (most recently revalidated: 768 passed, 35 skipped). No snapshot expectations changed.

## Phase 3 — Implement the minimal existential-atom representation, ownership, and display

**Status:** pending.

**Dependency:** the independently reviewable Phase 2 `TypeVarSet` representation refactor, existing PR 0, and merged PR 1B. The superseded `Atom::Range` refactor is not a dependency.

**Agreed scope:** this phase is a representation prerequisite. It does not add a production existential producer, consumer, or user-visible quantification behavior. Ownership and display are considered necessary parts of implementing the new representation, rather than optional future integrations. Every additional capability must earn its place by supporting an actual representation, ownership, or display requirement; presumed utility to a later phase is insufficient.

### Required representation

1. Represent an existential relation with its explicitly supplied `TypeVarSet` binder, its separately stored domain, and its separately stored body. Preserve the existing single-root `ConstraintSet` and existing range-specific IDs, payloads, arenas, and caches.
1. Mirror the existing range-constructor layering with private `NodeId`-level existential constructors. `ExistentialId::new(db, builder, locals, additional_domain, body)` first performs a shallow binder-order prepass calling `builder.intern_typevar` for each local, then computes `valid_specializations(locals) ∧ additional_domain` and delegates to a lower-level constructor such as `ExistentialId::new_with_domain(db, builder, locals, domain, body)`, which interns an already-complete domain and returns its `ExistentialId`. Cross-builder loading calls the complete-domain constructor directly, preserving the stored domain without recomputing or duplicating declared restrictions. Fold directly over the binder's instances in `ExistentialId::new`; do not add a single-use `TypeVarSet::valid_specializations` helper. Build declared domains in binder iteration order, place the additional domain after them using existing source-order-offset machinery, and preserve those offsets through ownership and loading. Binder interning does not walk declarations. Declared-domain construction evaluates the selected variables' own valid specializations but never recursively computes specializations for free variables mentioned in those declarations; any existing range-constraint interning triggered while constructing the domain remains unchanged.
1. `Existential::new_node(db, builder, locals, additional_domain, body)` returns a `NodeId`. For an empty binder, return `additional_domain ∧ body`, never merely `body`; this remains correct under an enclosing negation and therefore preserves universal implication semantics. Otherwise, call `ExistentialId::new` and create the corresponding existential-atom node. Do not implement any other existential-specific simplifications. An implementation TODO may mention the potentially sound future cases `∃X. never ∧ body → never` and `∃X. domain ∧ never → never`. Additional domains may mention outer/free variables; do not impose a locals-only invariant or introduce support discovery to enforce one. Document that callers are responsible for correctly scoped/fresh nested binder identities; do not add nested-binder traversal or debug assertions.
1. Do not add a higher-level constructor returning `ConstraintSet` until it has an actual caller. Its agreed future shape is a method on `ConstraintSet` whose `self` receiver is the body being quantified, and whose other inputs provide the quantified `TypeVarSet` and `additional_domain`; it returns the resulting `ConstraintSet`. The method name and exact argument/lifetime spelling can be decided when that caller exists.
1. Rename and generalize `ConstraintAssignment` to `AtomAssignment`, retaining the existing `Positive`, `Negative`, and `Unconstrained` variants and replacing each `ConstraintId` payload with `AtomId`; do not flatten atom kind and polarity into six variants merely to preserve the previous eight-byte layout. Reuse this atom-generic assignment for display clauses; implication/simplification delegates to existing range logic only when both atoms are ranges and otherwise treats existential atoms opaquely. Keep sequents, bound reasoning, and other genuinely range-specific operations strongly range-oriented, failing explicitly if they encounter an existential assignment.
1. Add only the generic atom discrimination, structural TDD handling, typed existential storage, and construction needed to represent and test that relation without changing existing production quantification behavior. Structural union, intersection, negation, and graph reconstruction may treat existential atoms opaquely. Generalize `for_each_unique_constraint` to `for_each_unique_atom`, but traverse only the current diagram's true/uncertain/false edges; ownership, loading, or display can explicitly recurse into existential domains/bodies when their contracts require it.
1. Existing short-circuiting `ConstraintSet::and` and `ConstraintSet::or` need not accept existential-containing sets: their range-only satisfiability prechecks may fail under the normal invariant. Use direct/non-short-circuiting structural composition where existential support is required. Replacing those prechecks with terminal-only checks is a separate performance investigation, not part of this phase.
1. Preserve existing ordinary-range behavior, deterministic ordering, source ordering, and performance-sensitive paths. Give atoms a total ordering by wobbled kind followed by reversed/wobbled per-kind ID; ordinary range constraints appear closer to the root under normal ordering, while wobble runs also exercise the opposite cross-kind order. Unsupported range-only semantic consumers must fail with an explicit invariant-violation panic when they encounter an existential atom, unless an operation is explicitly brought into scope by a concrete representation, ownership, or display requirement. Do not add proactive scans, automatic lowering, or speculative existential evaluation.

### Required ownership

1. `ConstraintSetBuilder::into_owned` must preserve each reachable existential relation, its binder, and its stored domain/body graphs in owned storage, including nested relations and shared subgraphs.
1. `OwnedConstraintSet::query` must read existential relations through its compacted-storage overlay and permit adding new existential relations without confusing retained IDs with newly allocated IDs.
1. `ConstraintSetBuilder::load` must rebuild existential relations and their domain/body graphs in another builder, preserve shared subgraphs, and permit adding additional existential relations afterward. Before rebuilding range constraints, iterate the owned existential arena and proactively intern each stored binder's explicit variables; the loop is naturally a no-op for ordinary range-only owned sets. Re-intern each relation through the complete-domain constructor rather than reapplying declared valid-specialization constraints to its already-combined stored domain.
1. Preserve the existing single-node load fast path only when that node is a range atom. An existential with a nonempty binder and terminal domain/body can also occupy exactly one interior node, and must use the ordinary existential-capable reconstruction path.
1. Remap builder-local node and payload IDs as required for ownership and loading. This is graph/arena remapping, not `ConstraintSet::apply_type_mapping_impl`: applying semantic `TypeMapping`s to existential binders, domains, or bodies is not an ownership requirement.
1. Leave `OwnedConstraintSet::types` unchanged unless an actual ownership, display, or regression test demonstrates that existential-specific changes are necessary; retained range constraints already expose the types appearing in existential domains and bodies.

### Required display

1. Both `ConstraintSet::display` and `ConstraintSet::display_graph` must support existential atoms while preserving existing ordinary-range output.
1. Concise display treats an existential as an opaque atom, analogous to an ordinary range constraint. Recursively format its own binder, domain, and body as `(∃ {locals} . {domain} ∧ {body})`; surrounding clause construction continues to combine positive and negative atom literals without deeply transforming, lowering, or simplifying the quantified formula.
1. Preserve the existing display-simplification pipeline rather than proactively skipping it or teaching it existential semantics. Mechanically generalize genuinely atom-generic display/literal operations as needed, and make atom implication/simplification return no range-containment or implication cases whenever an existential participates. Existing ordinary-range simplifications remain available.
1. Graph display must structurally represent the existential node by showing its quantified variables, inline and separately identified recursively rendered domain/body TDDs, and its ordinary true/uncertain/false branches. Embedded diagrams appear beneath their containing existential node, not in detached sections. Use shared graph numbering and shared-subgraph references rather than repeatedly expanding nodes. Do not reuse the opaque concise `(∃...)` rendering as the graph node's label. Resolve exact connector alignment and visual formatting iteratively without weakening these acceptance requirements.

### Regression coverage

Add only a small number of behavioral tests exercising the contracts of the constructors, ownership methods, display methods, structural Boolean operations, and explicitly unsupported semantic operations. Prefer observable rendered results and successful `query`/`load` round trips over assertions about arenas, IDs, caches, compaction layouts, traversal order, or other implementation details. Combine scenarios when practical and rely on existing tests for unchanged ordinary-range behavior; do not add an exhaustive edge-case matrix merely because an internal mechanism exists.

### Not automatically required

Do not implement semantic type mapping (`ConstraintSet::apply_type_mapping_impl`), capture avoidance, free-interface/support computation, existential lowering, receiver-constraint integration, production semantic interrogation, specialized simplification, or extra display machinery merely because a later consumer might require them. Do not assume that explicit free-variable-interface computation will ever be needed. Test-only lowering is deferred to PR 2B, where it can compare new existential traversal against the existing eager-quantification behavior; Phase 3 tests validate representation, ownership, and display structurally instead. Builder-local ID remapping required by `ConstraintSetBuilder::load` is part of the agreed ownership contract, but does not require applying semantic type mappings. Unsupported range-only semantic operations explicitly panic instead of guessing; exceptions must be justified and explicitly agreed. Add any other capability only after identifying a concrete acceptance case that makes it necessary for this phase.

**Open decisions:** refine exact graph-display connector alignment and identify any further explicitly justified exceptions to range-only invariant failures. Regression coverage should remain minimal and behavioral rather than prescribing storage or implementation details.

**Exit criteria:** existential relations have the agreed minimal representation, ownership, and display; existing range representations and behavior remain intact; production `reduce_inferable`, `for_all`, and mdtest quantifier entry points retain their current behavior; `ConstraintSet` has no companion domain; and all relevant tests pass. Do not expand these criteria without a concrete top-line requirement.

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

Run the affected constraint-ordering mdtest under its normal ordering when atom ordering, source order, or loading changes:

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

Alternative `TY_CONSTRAINT_SET_ORDER` settings are optional information-gathering tools, never a validation gate for any phase. Wobbled runs are expected to expose existing failures; do not require them to pass, do not update snapshots for them, and do not rewrite implementation or tests solely to make them green. Graph-display expectations may legitimately vary with TDD shape.

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
