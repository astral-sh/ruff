# PR 2A: scoped quantified-relation storage

## Status

- [x] PR 0's C0/E1–E6 behavior basis already exists on the parent stack.
- [x] PR 1B's visitor-driven `PathAssignments` implementation is already merged into `main`.
- [x] Phase 1 — Introduce single-variant `Atom` as a standalone, pure-refactoring PR.
- [ ] Phase 2 — Refactor `InferableTypeVars` to retain identity-keyed bound instances.
- [ ] Phase 3 — Integrate existential atoms, ownership, mapping, and display end to end.

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
    locals: InferableTypeVars<'db>,
    domain: NodeId,
    body: NodeId,
}
```

For this implementation, omit **only** the companion domain on `ConstraintSet`. PR 1A is not a prerequisite, and `ConstraintSet` retains its existing single root. A quantified relation does need its own domain because latent restrictions, such as receiver-binding constraints, belong to the quantifier's domain rather than its body. Its binder is the existing compact, Salsa-interned `InferableTypeVars` representation:

```rust
struct Existential<'db> {
    locals: InferableTypeVars<'db>,
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

`X` is exactly the explicitly supplied `InferableTypeVars` set. If `x`'s declared bound mentions an outer variable `y`, the generated domain can contain the relationship `x ≤ y`, but **must not** recursively include `valid_specializations(y)` unless `y` was explicitly included in `X`. Outer/free variables mentioned by declared bounds or `additional_domain` remain free and belong to the enclosing scope.

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

### The domain-construction obstacle

`InferableTypeVars` in `crates/ty_python_semantic/src/types/generics.rs` currently contains only `BoundTypeVarIdentity`s. An identity deliberately excludes the declared bound/constraints, so it cannot provide the instance needed by the existing:

```rust
BoundTypeVarInstance::valid_specializations(db, builder) -> NodeId
```

Change the existing Salsa-interned set representation to retain bound instances while still using their identity for membership and deduplication:

```rust
#[salsa::interned]
struct InferableTypeVarsInner<'db> {
    inferable: FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
}
```

This matches the existing representation of `GenericContext`. Instances with the same identity can be assumed to have equivalent bounds; differences can reflect whether bounds have been eagerly evaluated. When constructing or merging sets, deduplicate by identity with deterministic **first-wins** semantics. Keep lookup keyed by identity, rather than using an `FxOrderSet<BoundTypeVarInstance>` whose equality would incorrectly distinguish different instances of the same logical variable.

With instances available directly in `InferableTypeVars`, a quantified variable need not already occur in the body, `additional_domain`, or a builder-local arena before its declared domain is constructed. No additional canonical-instance storage or recovery mechanism is needed in `ConstraintSetStorage` or `OwnedConstraintSetInner`. Preserve fresh occurrence identities, `ParamSpec` attribute distinctions, Salsa sharing, insertion ordering, and existing identity-based membership.

### Existing range-only assumptions

`AtomId` currently indexes the arena of ordinary range constraints:

```rust
struct Constraint<'db> {
    typevar: BoundTypeVarInstance<'db>,
    bounds: ConstraintBounds<'db>,
}
```

Many paths assume every TDD atom has `.typevar` and `.bounds`, including:

- `SequentMap` generation and implication/disjointness checks;
- `PathAssignments` discovery and derived assignments;
- `PathBounds::compute` and its simple-conjunction fast path;
- eager existential abstraction and `remove_noninferable`;
- `satisfied_by_all_typevars` and constraint-support discovery;
- simplification and DNF/graph display;
- `apply_type_mapping_impl`;
- `ConstraintSetBuilder::into_owned`, `OwnedConstraintSet::query`, and `ConstraintSetBuilder::load`; and
- `OwnedConstraintSet::types`, used by receiver-constraint mapping in `signatures.rs`.

A quantified relation is an opaque Boolean atom at its containing TDD level. Range-only reasoning must recognize its kind and decline to derive range sequents or inspect nonexistent range bounds. However, ordinary Boolean TDD operations and source ordering must still treat it as a first-class atom.

### Atom-versus-constraint terminology

Once TDD nodes can contain different atom kinds, reserve **constraint** for the `Atom::Range` payload and use **atom** for data structures and operations shared by all node atoms. In particular, the pure refactor should rename:

- `ConstraintSetStorage::constraints` and `OwnedConstraintSetInner::constraints` to `atoms`;
- the generic `constraint_cache` to `atom_cache`;
- owned-storage `constraint_indices` to `atom_indices`;
- generic retained-index/overlay helpers such as `retained_constraint_index` and `adjusted_constraint_id` to their atom-named equivalents;
- generic node fields, lookup/interning helpers, traversal callbacks, and temporary maps from `constraint`/`constraints` to `atom`/`atoms` where they describe arbitrary TDD atoms.

Keep genuinely range-specific names such as `Constraint`, `ConstraintBounds`, bound-depth caches, range implication/intersection, and range-only sequent handling. Use `AtomId` for the shared TDD-atom index. Evaluate whether `ConstraintAssignment` should also gain an atom-oriented name; include that rename only if it improves consistency enough to justify its mechanical churn.

## Proposed representation

Introduce the atom discriminator in two separate, reviewable steps. First land an independently mergeable, behavior-preserving prerequisite PR with only the existing range variant:

```rust
enum Atom<'db> {
    Range(Constraint<'db>),
}
```

That refactoring must not add existential storage, change inference behavior, or modify mdtest expectations. A second independently reviewable refactoring PR changes `InferableTypeVars` to retain identity-keyed bound instances, but adds no declared-domain helpers, existential methods, or semantic behavior. Only after both prerequisites land does PR 2A add the existential variant and its compact payload:

```rust
enum Atom<'db> {
    Range(Constraint<'db>),
    Existential(Existential<'db>),
}

struct Existential<'db> {
    locals: InferableTypeVars<'db>,
    domain: NodeId,
    body: NodeId,
}
```

The names can change to match neighboring code. The essential properties are:

1. Existing range constraints remain cheap, copyable, and fast.
1. `AtomId` continues to identify a TDD atom and retain its ordering/wobbling behavior.
1. `InferableTypeVars` is already a compact, copyable, Salsa-interned set, so the quantified relation can be stored directly inside the existing interned atom arena; no `ExistentialId`, separate relation arena, boxed local slice, or parallel interning table is needed.
1. Locals are exactly the supplied `InferableTypeVars`, which now retain bound instances while preserving identity-based membership, first-wins deduplication, freshness distinctions, Salsa sharing, and efficient set representation.
1. The construction boundary accepts the authoritative `InferableTypeVars` set, an `additional_domain` constraint set, and a body. It stores that set unchanged and adds no variables discovered recursively in bounds.
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

## Phase 1 — Introduce single-variant `Atom` as a standalone, pure-refactoring PR

**Status:** complete.

**PR:** [astral-sh/ruff#27111](https://github.com/astral-sh/ruff/pull/27111) (`dcreager/refactor-atoms`).

**Dependency:** existing PR 0 and merged PR 1B only. This phase is intended to land as its own independently reviewable prerequisite PR, before the semantic PR 2A work.

1. Introduce `Atom<'db>` with exactly one variant, `Range(Constraint<'db>)`. Do not add `Existential`, an existential ID, a separate arena, a domain helper, or any other quantification feature in this revision.
1. Change builder storage, owned storage, and existing atom-interning caches from raw `Constraint` payloads to single-variant `Atom` payloads. Rename the corresponding `constraints` arenas to `atoms`, `constraint_cache` to `atom_cache`, and `constraint_indices` to `atom_indices` in the same mechanical refactor.
1. Rename the shared TDD-atom index to `AtomId`, and rename generic overlay/index helpers, interior-node fields, atom lookup/interning methods, traversal callbacks, and temporary collections to use atom terminology where they apply to arbitrary TDD atoms. Move range-atom construction to `Atom::new_range`, accepting optional lower and upper bounds. Audit `ConstraintAssignment` for a similarly useful rename, but avoid gratuitous churn.
1. Preserve arena ordering, atom identity, source order, overlay split points, hashing/equality, and terminal fast paths. Keep genuinely range-specific `Constraint`, `ConstraintBounds`, implication/intersection, bound-depth caches, and sequent terminology unchanged.
1. Migrate range-data access, TDD construction, sequent generation, path traversal, abstraction, solution extraction, type mapping, display, owned compaction, overlay queries, and cross-builder loading through exhaustive handling of `Atom::Range`.
1. Preserve the existing strongly typed range APIs and avoid introducing unreachable branches, panic-based accessors, or speculative handling for nonexistent variants. The refactor should be easy to verify mechanically: every existing atom is still precisely the same range constraint.
1. Verify existing unit tests, quantification mdtests, ordering/wobbling tests, graph-display tests, owned-storage tests, and the full crate suite without changing their expectations. Audit memory layout and hot-path behavior to avoid introducing measurable overhead for the one-variant enum.
1. Keep this revision separate from the `InferableTypeVars` representation change and from existential construction, so it can be extracted or landed as a pure-refactoring PR.

**Exit criteria:** `Atom::Range` is the only possible TDD atom; generic arenas/caches/indexes use atom-oriented names while range-specific logic retains constraint-oriented names; all existing behavior, snapshots, ownership semantics, ordering, and tests are unchanged; the revision is independently mergeable as a pure refactor.

**Validation note:** the normal full crate and mdtest suites pass. The reverse and XOR constraint-order wobble configurations retain pre-existing mdtest failures in `3954_recursive_protocol_structural_relation.md`, `constraint_set_ordering.md`, `constraints.md`, `quantification.md`, `implies_subtype_of.md`, and `typed_dict.md` (the affected subset varies by mask); both the failing-test sets and every normalized expected/actual diagnostic line were compared with the Phase 1 parent and are identical. No snapshot expectations changed.

## Phase 2 — Refactor `InferableTypeVars` to retain identity-keyed bound instances

**Status:** pending.

**Dependency:** Phase 1 in the implementation stack; this representation change is logically independent of the pure `Atom::Range` refactoring and should likewise be independently reviewable as a behavior-preserving prerequisite PR.

1. Change `InferableTypeVarsInner` from an ordered set of `BoundTypeVarIdentity`s to an ordered map from identity to `BoundTypeVarInstance`, following `GenericContext::variables_inner`.
1. Update the generic-context collector and mdtest tuple extraction to retain the instances they already encounter. Update the existing constraint-unit-test constructors to pass instances rather than first discarding them.
1. Preserve identity-based membership for both `BoundTypeVarIdentity::is_inferable` and `BoundTypeVarInstance::is_inferable`, including the caller that explicitly normalizes `ParamSpec` attributes before checking membership.
1. Update the existing `InferableTypeVars::merge` implementation to combine maps with stable insertion order and identity-keyed first-wins semantics. Test equivalent eager/lazy instances with the same identity, distinct freshness nonces, and distinct `P.args`/`P.kwargs` identities.
1. Update existing iteration and debug-display behavior as needed, without forcing evaluation of lazily represented bounds merely to construct, merge, or inspect the set. Do not add declared-domain helpers, existential constructors, new type-variable-set methods, or any other functionality required only by later phases.
1. Audit Salsa interning, tracked `merge`, existing cache keys such as `exists_cache`, identity-based ordering, and memory usage so equivalent instance representations do not introduce avoidable cache churn or behavior changes.
1. Add focused regression coverage for empty sets, identity-keyed first-wins construction and merging, eager/lazy equivalent instances, fresh identities, `ParamSpec` components, and preservation of lazy bounds. Do not add quantifier-domain or existential-specific tests in this refactoring revision.
1. Run the existing quantification and ordering mdtests without changing their expected behavior. Keep `ConstraintSetStorage`, `OwnedConstraintSetInner`, `Atom`, and existing existential-abstraction algorithms unchanged.

**Exit criteria:** `InferableTypeVars` retains actual bound instances while preserving its existing identity-based membership, ordering, first-wins deduplication, laziness, Salsa/cache behavior, and observable inference results; no new existential-specific methods or domain computations have been introduced; the revision is independently mergeable as a pure refactor.

## Phase 3 — Integrate existential atoms, ownership, mapping, and display end to end

**Status:** pending.

**Dependency:** the independently reviewable Phase 1 `Atom::Range` refactor and Phase 2 `InferableTypeVars` representation refactor.

Adding `Atom::Existential` immediately affects exhaustive matches, `ConstraintSet`, `OwnedConstraintSet`, type mapping, and display. Treat all of the workstreams below as one self-contained implementation phase and jj revision, not as independently passing revisions. If implementation reveals a genuinely self-contained intermediate boundary, update this plan before splitting the phase.

### Declared domains, existential atoms, and free interfaces

1. Add an `InferableTypeVars`-level helper that computes the conjunction of `BoundTypeVarInstance::valid_specializations` for exactly its stored instances in the supplied builder. Reuse the existing single-typevar implementation for unbounded variables, upper bounds, finite constraints, gradual materialization, and `ParamSpec` components; do not recursively add domains for variables mentioned in those bounds.
1. Support binders whose variables do not otherwise occur in the body, `additional_domain`, or builder. Their instances are available directly from `InferableTypeVars`; do not introduce canonical-instance recovery in builder or owned storage.
1. Preserve deterministic source ordering when constructing declared-domain roots. Never add a companion domain field to `ConstraintSet` or a separate top-level domain field to `OwnedConstraintSet`.
1. Extend the previously single-variant `Atom` with `Atom::Existential(Existential<'db>)`. Add a copyable `Existential<'db>` payload containing the supplied Salsa-interned `InferableTypeVars<'db>`, a stored domain `NodeId`, and a separate body `NodeId`. Intern it through the existing `Atom`/`AtomId` arena and cache, not through a separate quantified-relation arena.
1. Update existing range-specific consumers to distinguish `Atom::Range` from `Atom::Existential`. Keep ordinary range APIs strongly typed, handle existential atoms in structural operations, and panic with a clear invariant message if one unexpectedly reaches legacy satisfiability, validity, solution extraction, or other unsupported semantic consumers.
1. Add a private/internal constructor that takes an explicitly supplied `InferableTypeVars`, an `additional_domain: ConstraintSet`, and a body; verifies all constraint-set inputs use the same builder; retains the supplied type-variable set unchanged; computes `valid_specializations(locals) ∧ additional_domain`; and interns one existential atom.
1. Preserve the correct empty-binder fast path: with no locals, the quantified relation is `additional_domain ∧ body`, not merely `body`. Avoid unsupported simplifications that would accidentally assume a nonempty binder's domain is inhabited or flatten a nested scope.
1. Enforce binder invariants with debug assertions: stored bound instances match their identity keys, nested binder identities are fresh/disjoint where required, and scope is interpreted according to lexical nesting rather than TDD variable ordering.
1. Compute free support recursively from both the stored domain and body. Remove only the current binder's locals; preserve outer variables referenced through declared bounds or `additional_domain`, and preserve only the free interface of nested relation atoms.
1. Ensure support discovery walks a TDD as a DAG and does not recurse into a variable's bound merely to decide which variables are quantified. Cache computed support only if necessary and only in transient builder storage.
1. Give quantified atoms stable ordinary TDD source ordering and Boolean composition. Preserve deterministic ordering when combining declared valid-specialization roots with `additional_domain`. Test positive, negative, and uncertain outer edges structurally, including nested relation domains and bodies.
1. Keep existential constructors private/internal. Existing `ConstraintSet.exists`, `ConstraintSet.for_all`, `reduce_inferable`, and signature checking must continue to construct and use their current eager production representation; do not switch them to creating existential atoms in this PR.
1. Add declared-domain and structural tests for empty binders, unconstrained variables, upper-bounded variables, finite constrained variables, gradual bounds, `ParamSpec` attributes, unused binders, interning/deduplication, distinct binders/domains/bodies, nested scopes, shared domain/body DAGs, receiver-style additional equality constraints, contradictory additional domains, additional domains mentioning only outer variables, dependent declared bounds, and TDD ordering wobble.

### Explicit test-only lowering and production invariants

1. Add an explicit lowering operation for existential atoms constructed by Rust unit tests. Recursively lower nested domains and bodies, then replace each existential with the existing eager abstraction of `domain ∧ body` over its `locals`. If practical, compile this helper only for tests until a later production user exists.
1. Rebuild surrounding TDD structure while preserving positive/negative polarity, uncertain-branch semantics, source ordering, and shared subgraphs. Storage, Boolean structure, ownership, type mapping, and display must retain existential atoms unless a test explicitly requests lowering.
1. Never call lowering automatically from production satisfiability/validity checks, `satisfied_by_all_typevars`, solution extraction, `reduce_inferable`, `for_all`, or other semantic interrogation. Production does not construct existential atoms, so existing hot paths must pay no no-op scan, cache lookup, or normalization cost.
1. Panic with an explicit invariant-violation message if an existential nevertheless reaches a legacy semantic consumer. Add focused tests documenting the panic and tests that explicitly lower first before interrogating the equivalent ordinary constraint set.
1. Treat explicit test lowering as the existing eager algorithm with its existing limitations, not as correct scoped quantifier discharge. Do not interpret existential atoms as independent freely assignable Booleans or silently claim exact invariant projection.
1. Add unit tests comparing explicitly lowered positive, negative, nested, and uncertain existential formulas against manually constructing `domain ∧ body` and invoking the current eager abstraction. Include finite declared domains, receiver-style additional domains, free outer variables, and known incomplete invariant cases.
1. Leave switching production `reduce_inferable` to construct existential atoms as an optional, separately reviewable follow-up. That future PR may explicitly invoke lowering where required and must independently evaluate declared-domain behavior changes and performance.

### Owned storage, compaction, overlays, and loading

1. Extend the existing compacted atom storage to preserve inline quantified-relation payloads while retaining the existing terminal fast path, rank/index metadata, and `Arc` sharing. Do not introduce a separate quantified-relation arena or index space.
1. Update `ConstraintSetBuilder::into_owned` reachability traversal to mark an outer quantified atom, both its stored-domain and body nodes/atoms, and all nested quantified relations. Bound instances remain directly available from the relation's Salsa-interned `InferableTypeVars`.
1. Persist each relation's existing Salsa-interned `InferableTypeVars`, stored domain, and body directly in its atom, but never persist transient support caches or add a companion domain to `OwnedConstraintSet`.
1. Extend compacted-overlay atom access, retained-index lookup, identity-cache initialization, and atom interning so `OwnedConstraintSet::query` can read inline quantified relations and append new atoms after the existing overlay split.
1. Update `ConstraintSetBuilder::load` to rebuild nested domains and bodies before their containing atoms, reuse each relation's globally valid `InferableTypeVars` unchanged, preserve source-order offsets, and preserve sharing for repeated relation/domain/body DAGs.
1. Audit `OwnedConstraintSet::types`: it must expose types in quantified domains and bodies, bound instances retained directly by `InferableTypeVars`, and free interfaces introduced by declared bounds or `additional_domain`. If inspecting declarations requires `db`, update the small set of callers rather than losing mappings of relevant outer variables.
1. Add owned-storage tests for unreachable relation/domain/body compaction, nested scopes, shared domain/body subgraphs, sparse retained IDs, read-only overlay queries, mutation after overlay, cross-builder remapping, receiver-style additional domains, dependent declared bounds, fresh locals, and the storage-free terminal fast path.

### Type mapping, display, lexical scope, and deterministic ordering

1. Extend `ConstraintSet::apply_type_mapping_impl` to recursively rebuild both quantified domains and bodies and, when a mapping actually renames binder variables, construct the corresponding mapped `InferableTypeVars` set instead of treating a quantified atom as a range constraint.
1. Specify capture-avoiding binder behavior explicitly. Free/interface variables must be mapped; deliberate freshness/typevar-to-typevar renaming must update binder, domain, and body together; mappings that would replace a bound local with a concrete type must have one documented, tested policy rather than silently capturing or freeing that variable.
1. Ensure declared bounds of locals and additional-domain restrictions track mapped/freshened bound instances, including outer/free variables appearing only in the stored domain.
1. Extend concise constraint display and full graph display to show existential binders, their separate domains and bodies, negated polarity, uncertain branches, source order, and shared graphs. Do not introduce a separate universal atom or suggest that the outer `ConstraintSet` itself owns a domain.
1. Update range-clause implication/simplification so quantified literals compare opaquely and are never simplified using range containment or disjointness.
1. Preserve deterministic local order, TDD atom order, graph numbering, and source order across normal, reverse, and XOR builder orderings, cross-builder load, interning, and type mapping.
1. Add targeted unit tests for free-variable specialization, freshness renaming, nested binders, capture avoidance, local declared-bound mapping, mapped receiver-style additional domains, positive/negative/uncertain display, domain/body graph sharing, and ordering stability. Keep the PR 0 mdtest expectations unchanged unless a separately implemented behavior genuinely improves; annotate any changed intermediate expectations with clear TODOs.
1. Recheck the existing receiver-constraint `OwnedConstraintSet::types`/`Signature::map_constraints` integration and avoid exposing residual atoms through the mdtest `exists`/`for_all` entry points prematurely.

**Exit criteria:** existential atoms retain their explicit binders, combined domains, separate bodies, and correct free interfaces; they survive owned compaction, overlay queries, cross-builder loading, type mapping, and display; unit tests can explicitly lower them through the existing eager abstraction; unsupported production interrogation panics instead of silently guessing or performing a no-op normalization; production `reduce_inferable` and `for_all` do not create existential atoms; all new enum variants are handled end to end in the same revision; `ConstraintSet` has no companion domain; existing flat-TDD behavior, performance-sensitive fast paths, and all tests remain intact.

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
- Storing a companion domain on `ConstraintSet`, a globally deferred quantifier set, a boxed/builder-local binder set, a separate quantified-relation arena, or a cached free-interface field; storing the supplied `InferableTypeVars` and combined domain on `Existential` is required.
- Recursively adding declared domains for variables mentioned in a quantified variable's bound.
- Adding a `UniversalRelation` atom kind.
- Switching production `reduce_inferable`, `for_all`, signature comparison, or mdtest quantifier methods to construct residual existential atoms in this PR; a later, smaller PR may opt into that behavior and explicitly invoke lowering where needed.
- Automatically scanning for or lowering existential atoms on production semantic-query hot paths.
- Positive/nested path discharge, witness-family extraction, negative evaluation, incompleteness reporting, exact-cover optimization, receiver-specific quantifier partitions, or recursive-protocol feature work.
- Solving every C0/E1–E6 TODO in this storage-only PR; those tests are the contract for the later stack.
