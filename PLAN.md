# PLAN: Bound-typevar nonce freshening for generic callable occurrences

## Plan maintenance instructions

- Treat this file as the ground truth for the current implementation plan and handoff state.
- Keep status markers current as work proceeds:
    - `[ ]` not started
    - `[~]` in progress
    - `[x]` completed
    - `[?]` open question / needs decision
    - `[!]` known risk or blocker
- Before changing code, validate that the relevant status markers still match the repository state.
- When adding or completing steps, record the important details learned in this file.
- Larger phases should be completed in order unless the phase explicitly says it can proceed independently.
- Do not begin implementation until the Phase 0 nonce-shape questions are resolved or explicitly deferred.

## Goal

Implement alpha-renaming/freshening for generic callable type variables so that each generic callable occurrence contributes distinct inferable typevars.

Primary target: the TODO in `crates/ty_python_semantic/src/types/signatures.rs` in `TypeRelationChecker::check_signature_pair`.

## Current behavior and problem summary

- `InferableTypeVars` is currently an interned set of `BoundTypeVarIdentity<'db>`.
- `ConstraintSetBuilder` interns typevars by `BoundTypeVarIdentity<'db>`.
- `TypeRelationChecker::check_signature_pair` currently merges generic callable typevars into the ambient inferable set by raw identity, checks the signature pair, then existentially reduces those raw identities.
- This fails when the same source-level typevar has both:
    - an outer occurrence, such as `T` in a generic function body, and
    - an inner generic callable occurrence, such as recursively referencing `TypeOf[identity]` or directly calling `identity(t)`.
- The inner callable occurrence should have fresh inferable typevars; the outer occurrence should remain the surrounding typevar.

## Red/green mdtest coverage already added

The previous revision added currently-green TODO expectations in:

- `crates/ty_python_semantic/resources/mdtest/type_properties/implies_subtype_of.md`
    - invariant `list[T]` return type under surrounding constraints
    - recursive `listify` callable occurrence
- `crates/ty_python_semantic/resources/mdtest/generics/pep695/functions.md`
- `crates/ty_python_semantic/resources/mdtest/generics/legacy/functions.md`
    - recursive generic function calls
- `crates/ty_python_semantic/resources/mdtest/generics/pep695/callables.md`
- `crates/ty_python_semantic/resources/mdtest/generics/legacy/callables.md`
    - `partial(partial, drop)` higher-order callable occurrence

The implementation phase should eventually flip the TODO comments to desired expectations and remove the deliberately-undesired current expectations.

## Superseded TypeVarScheme direction

- [x] We considered a constraint-set-local `TypeVarScheme` that would allow duplicate `BoundTypeVarIdentity` entries in a scheme-local Vec.

- [!] That direction is probably not viable by itself. Raw `Type<'db>` values already contain `Type::TypeVar(BoundTypeVarInstance)`, and raw type operations can compare, simplify, union, intersect, display, or store those values before the constraint set can reinterpret them through a scheme. A scheme-only approach would need a parallel scheme-aware type representation to avoid collapsing duplicate raw identities. That is likely more invasive than adding freshness to bound typevar identity.

## Current proposed direction

Add a freshness nonce to bound typevar identity so freshened callable occurrences are represented as ordinary `Type::TypeVar` values with distinct identities.

Important points:

- The nonce should distinguish different occurrences of the same source-level generic context.
- A single fresh nonce is shared by all typevars bound by one generic callable occurrence; the nonce is per distinct generic context occurrence, not per typevar.
- Fresh copies should retain the original `TypeVarInstance`, binding context, paramspec attribute, display name, bounds, defaults, and variance behavior unless explicitly remapped.
- User-facing display should usually remain based on the source-level typevar (`T@f`), not the nonce. Debug/constraint display may need optional nonce disambiguation if fresh typevars leak.
- Once a type relation or call-inference check starts, every `Type::TypeVar` should already be unambiguous. The checker should not need to ask whether a raw `T` means the inner or outer binder.
- `TypeRelationChecker` should carry the nonce state for relation-driven freshening. Different unrelated nested generic callable occurrences inside one larger relation check must receive distinct nonces.

Recursive example target model:

```py
def identity[T](t: T) -> T:
    reveal_type(identity(t))
```

- The function body parameter `t` has type `T#outer`.
- The recursive callee occurrence `identity` is freshened with one new nonce to `(T#call) -> T#call`.
- Argument inference relates `T#outer ≤ T#call` and solves `T#call := T#outer`.
- The call result is `T#outer`, not `Unknown` and not leaked `T#call`.

## Phase 0: Resolve nonce-shape questions before implementation

- [x] Decide where the nonce is stored.

    Decision:

    - add a freshness/nonce field to `BoundTypeVarInstance<'db>`;
    - include that freshness field in `BoundTypeVarInstance::identity(db) -> BoundTypeVarIdentity<'db>`;
    - add the same freshness field to `BoundTypeVarIdentity<'db>` so equality/hash includes freshness.

    This makes all existing identity-based maps, sets, `GenericContext`, `InferableTypeVars`, and `ConstraintSetBuilder` treat fresh occurrences as distinct with minimal algorithmic changes.

- [x] Decide the nonce representation.

    Decision: use candidate 3, a local integer nonce generated by a freshness context. The freshness context for relation-driven freshening is the `next_nonce` state carried by `TypeRelationChecker`.

    Concrete representation:

    - define `TypeVarNonce` as a small wrapper around `NonZeroU32`;
    - non-fresh typevars store `None`, which is represented with niche optimization as `0` in `Option<TypeVarNonce>`;
    - freshened typevars store `Some(TypeVarNonce(nonzero))`;
    - `TypeRelationChecker` allocates fresh nonces monotonically from its `next_nonce` field, starting at 1;
    - one allocated nonce is used for all typevars in a single freshened generic context occurrence.
    - helper/generator types should drop any `Fresh` prefix: use names such as `TypeVarNonceGenerator`, not `FreshTypeVarNonceGenerator`.

    This is deterministic and avoids global allocation. It relies on the invariant that fresh typevars allocated from one checker are either solved/substituted away or existentially reduced before they can be combined with fresh typevars from an unrelated checker that might have reused the same local integer.

- [x] Decide the `TypeRelationChecker` nonce state shape.

    `TypeRelationChecker` methods generally take `&self`, and checkers are cloned or converted into related checker types. A plain copied `TypeVarNonce` could allocate the same nonce in two sibling/nested branches.

    Decision: `TypeRelationChecker` has a `next_nonce` field whose type is a clone-safe `TypeVarNonceGenerator`. The generator contains a single `Rc<Cell<TypeVarNonce>>` field, so `with_inferable_typevars`, `as_disjointness_checker`, and `as_equivalence_checker` all draw from the same sequence.

    Any freshening helper should request a single new nonce from the checker immediately before freshening a generic callable occurrence, then apply that same nonce to every typevar bound by that occurrence's generic context.

- [x] Decide how to represent a freshness origin/path.

    Decision: no separate origin/path is needed for relation-driven freshening. The checker-local `next_nonce` sequence is the freshness context and provides distinct identities for all fresh generic callable occurrences that can coexist within that relation check.

    Direct generic callable invocation must either reuse a checker-local nonce context or introduce an analogous per-call freshness context with the same non-escape invariant.

- [x] Decide which comparisons should include freshness.

    Decision: all comparisons of bound typevar identities include freshness. This follows from storing the nonce in `BoundTypeVarIdentity`; any equality/hash/order operation over bound typevar identities should take the nonce into account.

    If code truly needs the source-level logical typevar ignoring freshness, add an explicit helper such as `source_identity_ignoring_freshness` instead of weakening `is_same_typevar_as`.

- [x] Decide how freshening interacts with bounds, constraints, and defaults.

    Decision: the type mapping that performs freshening should recurse through type structure like any other type mapping. When it freshens a generic context occurrence, it should also apply to the bounds, constraints, and defaults of the typevars being freshened. This naturally rewrites sibling typevar references in those attributes to the same fresh generic-context occurrence nonce.

    This means the freshening helper should not merely copy the original `TypeVarInstance` unchanged if doing so would leave stale outer references in bounds/defaults. It should construct fresh bound typevars whose underlying typevar attributes have been mapped through the same occurrence mapping.

    The existing `TypeMapping` / `apply_type_mapping` infrastructure should take care of most of this recursion for us, or at least provide an easy way to ensure that typevar attributes are mapped consistently with the signature/parameter/return types.

    Implementation may need a two-pass freshening helper: first create fresh shell variables for every typevar in the generic context using the shared nonce, then build the occurrence mapping, then map each fresh variable's bounds/constraints/defaults through that occurrence mapping. Because shell variables and final variables can have different `BoundTypeVarInstance` salsa IDs while sharing the same `BoundTypeVarIdentity`, freshening substitutions should match by bound typevar identity, not exact `BoundTypeVarInstance` equality.

- [x] Decide ParamSpec / TypeVarTuple scope.

    Decision: ParamSpecs and TypeVarTuples are represented by `TypeVarInstance` and related bound-typevar types, so they should automatically be subject to the same freshening mechanism. This is desired. Freshening should preserve `paramspec_attr` for `P.args` / `P.kwargs` views while applying the occurrence nonce consistently.

## Phase 1: Add nonce scaffolding without behavior changes

- [x] Add a freshness field to `BoundTypeVarInstance` with a default/non-fresh value.
- [x] Add the freshness field to `BoundTypeVarIdentity`.
- [x] Update all constructors to provide the default freshness.
    - `with_binding_context`
    - `synthetic`
    - `synthetic_self`
    - `with_paramspec_attr`
    - `without_paramspec_attr`
    - `map_bound_or_constraints`
    - materialization / `to_instance` helpers
- [x] Add helpers:
    - `freshen(db, nonce) -> BoundTypeVarInstance`
    - generated `freshness(db)` accessors from the salsa field
    - no `identity_ignoring_freshness` or display changes were needed for this phase
- [x] Preserve current mdtests before any call/signature behavior changes.
- [x] Add clone-safe nonce state to `TypeRelationChecker` and propagate it through checker constructors/conversions.
    - direct `TypeRelationChecker` construction in `relation.rs`
    - `with_inferable_typevars`
    - `as_disjointness_checker`
    - `as_equivalence_checker`
    - conversions from `DisjointnessChecker` / `EquivalenceChecker` back into `TypeRelationChecker` carry the same generator handle

Phase 1 implementation notes:

- `TypeVarNonce` is a public wrapper around `NonZeroU32`; `None` remains the non-fresh/default value in `Option<TypeVarNonce>`.
- `TypeVarNonceGenerator` uses `Rc<Cell<TypeVarNonce>>`, so cloned relation/disjointness/equivalence checkers share one monotonically increasing sequence. Exhaustion panics; supporting more than 2 billion distinct fresh occurrences in one relation context is intentionally out of scope.
- Constructors that copy an existing `BoundTypeVarInstance` preserve its freshness; constructors that synthesize or initially bind a source typevar use `None`.
- User-facing `BoundTypeVarIdentity` display still ignores freshness.
- Validation run after Phase 1:
    - `cargo check -p ty_python_semantic`
    - targeted mdtests for `generics/pep695/functions.md`, `generics/legacy/functions.md`, `generics/pep695/callables.md`, `generics/legacy/callables.md`, and `type_properties/implies_subtype_of.md`

## Phase 2: Add a reusable generic-context freshening helper

Goal: freshen all typevars bound by a generic callable occurrence as a group, using one shared nonce for the whole occurrence.

- [x] Add a `TypeMapping` variant that takes a `GenericContext<'db>` and one `TypeVarNonce`, deriving on demand:
    - fresh `GenericContext<'db>` via `TypeMapping::update_signature_generic_context`;
    - on-the-fly mapping from original `BoundTypeVarInstance`/identity to fresh `BoundTypeVarInstance`;
    - fresh identities for inferable/existential handling by applying the mapping to the original context.
- [x] Apply that mapping to a `Signature<'db>` / `CallableSignature<'db>`.
    - The freshening path is a `TypeMapping` variant, not an `ApplySpecialization` variant, because freshening a matching typevar must recursively apply the same type mapping to that typevar's bounds/constraints/defaults.
    - The mapping matches by `BoundTypeVarIdentity` / `is_same_typevar_as`, not exact `BoundTypeVarInstance`, so it handles materialized variables correctly.
- [x] Ensure freshening rewrites only typevars bound by the callable occurrence's generic context.
    - Free outer typevars must remain unchanged.
- [x] Audit nested callable signatures.
    - A nested generic callable occurrence should get its own fresh group when it is itself checked/invoked, not merely because it appears inside a freshened outer signature.

Phase 2 implementation notes:

- Added `TypeMapping::FreshenBoundTypeVars { generic_context, nonce }`, which replaces matching typevars by `BoundTypeVarIdentity` rather than exact `BoundTypeVarInstance` equality.
- `TypeMapping::update_signature_generic_context` now preserves generic contexts under `FreshenBoundTypeVars` by applying the same mapping to the context variables.
- There is no separate `GenericContextFreshening` wrapper; callers should construct/use the `TypeMapping` variant directly.
- `GenericContext::contains(db, bound_typevar)` performs efficient membership checks via `variables_inner(db).contains_key(...)`, normalizing ParamSpec attrs before lookup.
- `BoundTypeVarInstance::apply_type_mapping_impl` owns the freshening behavior: when the typevar belongs to the generic context, it freshens the bound typevar and recursively applies the same type mapping to the underlying typevar's bounds/constraints/defaults so sibling references point at the same fresh occurrence identity. Repeated on-demand construction relies on salsa interning to canonicalize equivalent fresh variables.
- The old shallow `BoundTypeVarInstance::freshen` helper was inlined into the recursive `freshen` helper.
- The helper only maps identities from the generic context being freshened. Nested callable signatures are traversed like normal type structure for those identities, but nested generic callable occurrences are not independently freshened until Phase 3/4 call or relation boundaries.
- Validation run after Phase 2:
    - `cargo check -p ty_python_semantic`
    - targeted mdtests for `generics/pep695/functions.md`, `generics/legacy/functions.md`, `generics/pep695/callables.md`, `generics/legacy/callables.md`, and `type_properties/implies_subtype_of.md`

## Phase 3: Freshen direct generic callable invocation

Goal: fix recursive direct calls such as `identity(t)` inside `identity`.

- [x] Locate where call binding receives a generic callable signature before inferring specialization.
- [x] Freshen the callable signature's generic context at the call occurrence boundary.
    - Implemented for regular callable bindings whose generic contexts do not contain ParamSpecs.
    - Constructor bindings and ParamSpec-containing generic contexts are intentionally deferred; enabling them exposed broader constructor/ParamSpec inference regressions unrelated to the recursive TypeVar target.
- [x] Add an analogous per-call nonce source if direct call binding cannot reuse a `TypeRelationChecker` nonce generator.
- [x] Run existing specialization inference against the fresh signature.
- [x] Ensure solved fresh typevars are substituted into the return type and do not leak for the recursive TypeVar cases.
- [x] Verify recursive function mdtests in both PEP 695 and legacy files.

Phase 3 implementation notes:

- `Bindings::match_parameters` now creates a per-call `TypeVarNonceGenerator` and freshens regular callable bindings before argument type-context inference and specialization.
- `CallableBinding` allocates one nonce for the callable occurrence and applies it to all overload generic contexts in that binding, so overloads in one callable occurrence share the same occurrence nonce while still remaining distinct by source identity/binding context.
- Generic contexts containing ParamSpecs are skipped for now. A trial implementation that freshened ParamSpecs caused existing `Callable[P, R]` inference cases such as `accepts_callable(returns_int)` to lose the inferred `P` specialization.
- Constructor bindings are skipped for now. A trial implementation that freshened constructors caused existing generic class constructor inference cases such as `list(items)` to leak unsolved class typevars.
- Validation run after Phase 3:
    - `cargo check -p ty_python_semantic`
    - targeted mdtests for `generics/pep695/functions.md`, `generics/legacy/functions.md`, `generics/pep695/callables.md`, `generics/legacy/callables.md`, and `type_properties/implies_subtype_of.md`

## Phase 4: Freshen generic callable-vs-callable relation

Goal: fix the TODO in `TypeRelationChecker::check_signature_pair`.

- [ ] In `check_signature_pair`, allocate a single nonce from `self` for the source signature and freshen the source signature's generic context when present.
- [ ] Freshen the target signature's generic context independently with a separate single nonce when present.
- [ ] Merge ambient inferables with the fresh source/target inferables.
- [ ] Call `check_signature_pair_inner` on the freshened signatures.
- [ ] Existentially reduce exactly the fresh source/target identities after the inner check.
    - Do not reduce an outer/non-fresh identity with the same source-level typevar.
- [ ] Verify `type_properties/implies_subtype_of.md` recursive and invariant `listify` cases.

## Phase 5: Validate higher-order and overload cases

- [ ] Verify `partial(partial, drop)` in both PEP 695 and legacy syntax.
- [ ] Verify overloaded callable inference still treats overload sets as intended.
- [ ] Verify generic callable factory tests still pass.
- [ ] Audit `SpecializationBuilder::add_type_mappings_from_constraint_set` for any accidental fresh-typevar leaks.
- [ ] Add debug assertions if fresh typevars should never appear in final public result types after a call/relation path.

## Phase 6: Flip mdtest expectations and add final regression coverage

- [x] Flip direct recursive function reveal-type expectations in the PEP 695 and legacy function mdtests.

- [ ] Flip remaining TODO expectations added in the previous revision:

    - `partial(partial, drop)` reveal types
    - invariant `listify` implication assertions
    - recursive `listify` implication assertion

- [ ] Remove the remaining deliberately undesired current expectations and `static-assert-error` comments.

- [ ] Consider additional coverage after the core cases pass:

    - generic defaults that reference sibling typevars;
    - ParamSpec callable occurrences;
    - overloaded generic callable occurrences with repeated source-level typevar identities;
    - nested generic callable signatures.

- [ ] Run targeted mdtests:

    ```sh
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::generics/pep695/functions.md
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::generics/legacy/functions.md
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::generics/pep695/callables.md
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::generics/legacy/callables.md
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::type_properties/implies_subtype_of.md
    ```

- [ ] Run broader ty semantic tests after targeted mdtests pass.

- [ ] Run `/home/dcreager/bin/jpk run -a` before final handoff.

## Phase 7: Cleanup and documentation

- [ ] Remove obsolete TODO in `signatures.rs` or replace it with narrower follow-up TODOs.
- [ ] Audit comments in `constraints.rs` and `generics.rs` that describe typevar identity/order.
- [ ] Document the distinction between source-level typevar identity and fresh bound-typevar occurrence identity.
- [ ] Add or update debug display for fresh typevars if needed.

## Notes for future implementers

- Do not use `git`; this worktree uses `jj`.
- Create a new `jj` revision before code edits (`jj new -A @`) and describe it with a `[π]` prefix.
- Prefer small, behavior-preserving migration phases. First add nonce plumbing with default freshness, then enable freshening at one occurrence boundary at a time.
- Be especially careful with any method that accesses AST `.node()`; ty requires those methods to be `#[salsa::tracked]` for incrementality.
