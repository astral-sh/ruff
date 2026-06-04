# Rescope still-generic typevars in returned callables

## Maintenance instructions

- Treat this file as the ground-truth handoff plan for this feature.
- Keep status markers up to date:
    - `[ ]` not started
    - `[~]` in progress
    - `[x]` complete
    - `[!]` blocked / needs decision
- When resuming work from this plan, first re-read the relevant code and tests to validate that
    the status markers are still accurate before continuing.
- Follow the repository source-control rules: work in dedicated `jj` revisions, describe each with
    a `[π]` prefix, and use `jj diff --git` / `jj diff --stat` for review.
- Prefer self-contained revisions that correspond to coherent phases of the plan. Finer-grained
    revisions are fine when that produces cleaner review history, but every revision must stand on its
    own: tests pass, snapshots are reviewed, relevant hooks/lints are applied, and there are no
    unexplained red mdtests.
- If investigation shows that a behavior should be deferred to a later phase or a follow-on PR, add
    explicit TODO/xfail mdtest comments documenting the known limitation before considering the
    current revision complete. For valid rebinding examples we should generally aim for `TODO: no error`, but if false-positive errors remain after rebinding, investigate whether they are due to
    this feature or an existing inference/specialization gap before deciding whether to fix or punt.
- All code changes must be tested. After running mdtests with snapshot update variables, review any
    changed snapshots or pending snapshots.

## Problem summary

Issue: <https://github.com/astral-sh/ty/issues/3642>

When a generic higher-order call returns a `Callable`, the specialized call result can still contain
unsolved type variables in the returned callable signature. Today those typevars remain scoped to
some earlier context, so the callable is revealed with free typevars and later calls cannot infer a
fresh specialization.

Example:

```py
def higher[A, B](f: Callable[[A], B]) -> Callable[[A], B]: ...
def identity[T](x: T) -> T: ...

reveal_type(higher(identity))
```

Current behavior is effectively:

```py
(A@higher, /) -> A@higher
```

Expected behavior is a returned generic callable:

```py
[A](A, /) -> A
```

This allows each later call to specialize independently:

```py
higher(identity)(1)    # Literal[1]
higher(identity)("x")  # Literal["x"]
```

The parent revision `mqnuvxrs` added red mdtests in:

- `crates/ty_python_semantic/resources/mdtest/generics/pep695/callables.md`
- `crates/ty_python_semantic/resources/mdtest/generics/legacy/callables.md`

## Existing similar logic

`GenericContext::remove_callable_only_typevars` in
`crates/ty_python_semantic/src/types/generics.rs` handles a related definition-time case:

- It runs while building a public function signature.
- It finds typevars that appear only in return-position `Callable`s and nowhere else in the
    function signature.
- It moves those typevars from the function's generic context to the returned callable's generic
    context.
- It creates renamed typevars with a `'return` suffix and removes the originals from the function's
    own generic context.
- It intentionally filters to typevars bound by the function whose signature is being processed, so
    it does not steal typevars from enclosing class/function contexts.

The current implementation explicitly does **not** implement nested-callable ownership: its visitor
stores only one `in_callable_type` and comments that it "only consider[s] the outermost Callables in
the return type." Typevars inside nested returned callables are attributed to that outermost
callable. The moved shared implementation should generalize this for **both** use cases: the
existing signature-based rebinding shim and the new call-result rebinding should use the same
innermost-cover behavior, not diverge.

The new feature is similar because it also needs to detect typevars inside returned callables and
attach a generic context to those callables. It is different because it must run after call
specialization, and the typevars to rescope can come from:

- the callee's generic context, if they remain unsolved, and
- generic callables passed as arguments, whose callable-local typevars can flow into the result.

Therefore the new logic must not use the `binding_context.definition() == function_definition`
filter from `remove_callable_only_typevars`.

## Design decisions and constraints

- Rescoping should happen on the final call result type, after generic call specialization has been
    applied.
- Eligibility must be driven by an explicit candidate set, not by a purely structural search for
    all typevars that appear only inside returned callables. This avoids accidentally capturing
    enclosing generic typevars.
- The candidate set should be efficient to compute as:
    - the call's `inferable` typevar set, unioned with
    - the deferred-quantification set on the constraint set produced during specialization
        inference, unioned with
    - generic-context typevars mentioned by types assigned during specialization inference. This
        covers generic callable arguments that flow into a result through an intermediate typevar,
        such as `drop` in `partial(partial, drop)`, rather than directly through a callable
        relation.
        This should use the actual `self.inferable_typevars` for the call, even though that can include
        typevars from enclosing class contexts for method calls; add/keep tests to guard against
        over-rebinding class/enclosing correlations.
- Do not rely on the typevar's source definition being the called function. Eligible typevars often
    come from generic callables passed as arguments.
- Returned-callable traversal must not ignore nested returned callables. For example, in a result
    shaped like `Callable[[], Callable[[T], T]]`, `T` is still eligible for rescoping modulo the other
    conditions.
- A candidate typevar should be rebound by the innermost returned callable that covers all of its
    eligible occurrences.
    - In `tuple[T, Callable[[T], T]]`, `T` is not eligible because one occurrence is outside any
        returned callable.
    - In `Callable[[], Callable[[T], T]]`, `T` is rebound by the inner callable:
        `() -> [T'return](T'return) -> T'return`.
    - Conceptually, in `Callable[[], tuple[T, Callable[[T], T]]]`, `T` would be covered by the outer
        callable because the tuple occurrence is outside the inner callable but still inside the outer
        callable. However, this shape is analogous to `def f[T]() -> T: ...`: there is no way to infer
        `T` from a non-callable return value. Treat this as design intuition only, not as a required
        test target for this PR.
- Only callables that are themselves in returned-value positions should create nested ownership
    frames. At the top-level return type, callables found anywhere structurally inside containers,
    unions, intersections, etc. are returned values and should create frames. Once traversing a
    callable signature, callables inside parameter annotations do not create frames; callables inside
    return annotations do. A callable nested in a returned callable's parameter type should not bind
    typevars away from the surrounding returned callable. For example,
    `Callable[[Callable[[T], T]], int]` should become
    `[T'return]((T'return, /) -> T'return, /) -> int`, not
    `([T'return](T'return, /) -> T'return, /) -> int`.
- If a candidate typevar appears both inside a returned callable and outside that callable in the
    surrounding result type, that callable cannot bind the typevar. An enclosing returned callable
    may still bind it if all occurrences are inside that enclosing callable.
- If a `CallableType` is already generic, do not create an ownership frame for it and do not rewrite
    it. Treat a callable as already generic if any overload signature has a `generic_context`. Still
    traverse its parameter and return types to discover candidate typevar occurrences; those
    occurrences belong to the current enclosing frame, if any. Do not count the callable's existing
    generic-context metadata itself as an occurrence. Do not add an extra filter for typevars bound by
    the existing generic context unless a concrete use case requires it; the eligibility filter should
    usually exclude irrelevant typevars already.
- Avoid over-rescoping typevars from enclosing contexts. A typevar from an outer generic function or
    class that is used as a concrete specialization should remain free in the result expression, not
    become callable-local.
- Preserve ParamSpec handling by normalizing `P.args` / `P.kwargs` to their base `P` when collecting
    generic-context variables.
- Preserve existing signature traversal behavior by not traversing callable-signature parameter
    default types. Top-level function parameter defaults are still included as outside roots only
    because that is existing `remove_callable_only_typevars` behavior.
- Use `BoundTypeVarIdentity` for internal ownership sets/maps, and keep a representative
    `BoundTypeVarInstance` only where needed for rewriting. Preserve stable ordering based on first
    occurrence; any stable order would be acceptable for reproducible tests, and first occurrence is
    the easiest to implement.
- The shared implementation should manually traverse callable signatures instead of relying on
    `walk_callable_type` / `walk_signature`, because those helpers do not distinguish parameter types
    from returned-value positions and also visit signature generic-context metadata as ordinary
    typevar occurrences. It should similarly traverse function types manually for candidate
    occurrences, but never create ownership frames for function types and never rewrite them.
- Implement the refactor first using only the existing signature-based rebinding path. Validate that
    existing signature rebinding tests still pass before adding candidate tracking and the new
    inference-based call-result rebinding.

## Proposed implementation

### Revision guidance

A clean revision split would be:

1. `[π] Refactor returned-callable typevar rebinding`
    - Move shared code to `types/typevar/return_callables.rs`.
    - Keep only the signature-based shim wired.
    - Adopt innermost-cover behavior for signature-based rebinding.
    - Update/add mdtests for signature-based behavior.
    - All affected mdtests pass.
1. `[π] Track returned-callable rescoping candidates`
    - Make `ConstraintSet::deferred_quantification` `pub(crate)`.
    - Add candidate capture plumbing in `SpecializationBuilder` / call binder as needed.
    - Ideally no behavior change yet, or all affected tests still pass.
1. `[π] Rescope call-result returned callables`
    - Wire the shared helper into `ArgumentTypeChecker::infer_specialization`.
    - Update the parent revision's red mdtests to green.
    - Add regressions for class/enclosing contexts and nested call-result callable behavior.
    - All affected mdtests pass.

Finer-grained revisions are fine if they are cleaner while implementing, but every revision must be
self-contained.

### Phase 1: Establish the signature-rebinding baseline

Goal: before doing any new inference-based work, verify the existing signature-based rebinding
behavior so the refactor can be validated independently.

- [x] Run the mdtests that already exercise `remove_callable_only_typevars` and related callable
    rebinding behavior. The parent revision contains new red/green cases in these same files, so note
    which sections are expected to remain in their current pre-feature state during the refactor.

    ```sh
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always \
      CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
      cargo test -p ty_python_semantic --test mdtest -- generics/pep695/callables.md

    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always \
      CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
      cargo test -p ty_python_semantic --test mdtest -- generics/legacy/callables.md
    ```

- [x] Identify the existing signature-based rebinding sections, especially:

    - `Naming a generic Callable: type aliases`
    - `Naming a generic Callable with paramspecs: type aliases`
    - `Naming a generic Callable: function return values`
    - `Naming a generic Callable with paramspecs: function return values`

- [x] Do not start inference-based call-result rebinding until the signature-only refactor below is
    complete and these existing tests still pass.

### Phase 2: Refactor signature-based rebinding only

Goal: move and generalize the existing implementation while preserving the current public entry
point and keeping the change scoped to signature-based rebinding.

- [x] Move the refactored implementation out of `GenericContext` into a typevar-focused submodule,
    likely `crates/ty_python_semantic/src/types/typevar/return_callables.rs`, with
    `types/typevar.rs` declaring `mod return_callables;` and re-exporting the crate-visible API as
    needed.
- [x] Leave `GenericContext::remove_callable_only_typevars` behind as a compatibility shim around
    the moved/refactored implementation, so existing callers do not need to change shape.
- [x] Prefer a single exported plain (not salsa-tracked) function from the new module whose behavior
    is customized by an `Fn(BoundTypeVarInstance<'db>) -> bool` eligibility filter callback, instead
    of exposing multiple near-duplicate entry points or a rigid enum. In this phase, only wire the
    existing signature-based use.
- [x] Reuse as much of the `remove_callable_only_typevars` implementation as practical. The goal is
    code reuse, not merely reproducing behavior in a separate implementation. Renaming rebound
    typevars with a `'return` suffix should fall out of sharing the existing rescoping machinery; any
    switch from renaming to pure nonce freshening should be separate follow-on work that changes all
    uses of this logic together.
- [x] Replace the existing outermost-only callable tracking with the shared innermost-cover
    algorithm, and apply that updated behavior to signature-based rebinding too. Do not preserve the
    old outermost-only behavior just for the shim.
- [x] Manually traverse callable signatures instead of relying on `walk_callable_type` /
    `walk_signature`, because those helpers do not distinguish parameter types from returned-value
    positions and also visit signature generic-context metadata as ordinary typevar occurrences.
- [x] For the signature-based shim:
    - keep the existing caller-side fast path that returns immediately when `generic_context` is
        `None`; avoiding unnecessary traversal is the caller's responsibility.
    - pass function parameter annotations/defaults as plain `Type` outside roots with no active
        callable frame, so any typevar occurrence there prevents callable rebinding for that typevar.
        Include default types because that is the existing behavior; avoid unnecessary churn.
    - traverse the function return annotation as the returned-value root where callable ownership
        frames can be created;
    - pass an eligibility callback equivalent to the existing filter that signature-based rebinding
        only moves typevars bound by the function definition being processed;
    - return a tuple containing the rewritten return type and an `FxIndexMap` of original rebound
        typevars keyed by `BoundTypeVarIdentity`; keep the callable replacement map internal. The
        signature shim can use the returned map for membership checks while removing typevars from the
        function's generic context, avoiding an additional set allocation. A named result struct is
        unnecessary.
- [x] Implement the shared traversal with a postorder callable-frame algorithm:
    - Maintain active frames for returned callable occurrences. Each frame stores the `CallableType`,
        a set/map of candidate typevars seen directly in that callable frame, and completed nested
        callable frames.
    - When entering a callable visitor in a returned-value position, push a fresh frame. When leaving
        that callable visitor, pop the frame and move the completed frame into the parent frame (or into
        top-level results if it has no parent). Do not leave completed child frames on the active stack;
        otherwise sibling occurrences could be incorrectly attributed to the child.
    - At the root returned type, callables discovered anywhere structurally inside containers, unions,
        intersections, etc. are returned values and can create frames.
    - A nested callable creates its own frame only when it is part of the value returned by the
        containing callable and does not already have a generic context. Callables that occur in
        parameter types do not create frames; their candidate typevar occurrences are attributed to the
        current callable frame.
    - A callable that is already generic also does not create a frame and is never rewritten. Treat
        this as true if any overload signature has a `generic_context`. Still manually traverse its
        parameter and return types so candidate typevars inside its signature are discovered and
        attributed to the current enclosing frame. Do not count its existing generic-context metadata as
        an occurrence. Do not add a special ignore set for the callable's own generic-context variables
        unless a concrete use case requires it.
    - For `FunctionLiteral` / function types, preserve existing traversal behavior by manually
        traversing parameter and return types for candidate occurrences, but do not create frames, do
        not rewrite function types, and do not count signature generic-context metadata as an
        occurrence.
    - For each candidate typevar occurrence, normalize ParamSpec attrs to the base `P` before calling
        the eligibility filter, record a representative normalized `BoundTypeVarInstance` keyed by
        `BoundTypeVarIdentity` in a global `FxIndexMap` on first occurrence, and add the identity to the
        current active frame. If there is no active frame, add the identity to a global ineligible set
        because it appears outside any returned callable. Match existing behavior by not traversing the
        bounds/defaults/constraints attached to the typevar occurrence itself.
    - When finalizing a callable frame, first promote any typevar that appears in two or more nested
        callable frames into the current frame. Then, for every typevar in the current frame, remove it
        from all nested callable frames. This gives the innermost callable that covers all occurrences.
        When building replacement maps, iterate the global representative `FxIndexMap` and select
        identities owned by the frame so generic-context display order follows stable first-occurrence
        order rather than hash/set or child traversal order.
    - After traversal, discard globally ineligible candidates from all frame results.
    - Do not traverse parameter default types inside callable signatures, matching existing
        `walk_signature` behavior. Top-level function parameter defaults are still outside roots for
        the signature-based shim because that is existing behavior.
    - Keep `should_visit_lazy_type_attributes() == false`, but preserve the existing special-case
        recursion into PEP 695 and manual PEP 695 type aliases in both outside roots and returned-value
        roots so aliases can make typevars ineligible and returned callables hidden behind aliases are
        still discovered.
- [x] In finalization, for each callable:
    - keep only typevars assigned to that callable by the innermost-cover rule and accepted by the
        use-case-specific filter;
    - skip callables that already have a generic context, which should not have frame results in the
        first place;
    - build the old-to-new renamed typevar replacement map using the existing `'return` suffix logic;
    - rewrite the callable signature with the replacement map;
    - build `GenericContext::from_typevar_instances` for the renamed typevars;
    - attach it with `CallableSignature::with_inherited_generic_context`; and
    - replace the callable in the result type using the existing `TypeMapping::RescopeReturnCallables`
        mechanism, preserving existing alias/rewrite behavior rather than manually rebuilding and
        potentially expanding aliases. Keep the callable replacement map as `FxHashMap` unless the
        implementation naturally needs a different internal collection; stable ordering is needed for
        typevar replacement/generic-context construction, not callable replacement lookup.

### Phase 3: Validate the signature-only refactor

- [x] Rerun the callable mdtests from Phase 1 and verify that existing signature-based rebinding
    behavior still passes.
- [x] Review any changed snapshots. Snapshot updates are allowed in this phase, but only when they
    are attributable to the intentional innermost-cover generalization for signature-based rebinding,
    not to the new inference-based feature. Investigate any broader churn.
- [x] If needed, add focused signature-only regressions for nested returned callables to lock in the
    shared innermost-cover rule before wiring call-result rebinding.

### Phase 4: Add candidate tracking for call-local typevars

Goal: identify typevars that are eligible for call-result returned-callable rescoping without
accidentally capturing enclosing generic typevars. If specialization inference retries after
discarding an initial return-context-preferred builder, use only the final builder's candidate set;
it must match the builder whose specialization is actually applied to the call result.

- [x] In `crates/ty_python_semantic/src/types/constraints.rs`, make
    `ConstraintSet::deferred_quantification` `pub(crate)` rather than adding an accessor method.
    This lets call-specialization code read which generic-callable typevars were existentially
    introduced while comparing callable signatures; hiding this behind a method is YAGNI for now.
- [x] In `SpecializationBuilder` (`types/generics.rs`), define the rescoping candidate set as
    the call's `inferable` set unioned with the deferred-quantification set of the constraint set
    created during specialization inference and with generic-context typevars mentioned by assigned
    specialization types. Use `self.inferable_typevars` for the inferable side, even though it can
    include enclosing class typevars for method calls. In practice, deferred quantification can
    likely be read from `self.pending` after argument/callable inference has intersected in all
    relevant constraints.
- [x] Expose that candidate set to the call binder, or return it alongside the built
    specialization. Make sure the exposed set comes from the final specialization attempt, not from
    any abandoned return-context-preferred attempt. Capture it immediately before calling
    `builder.build_with(...)`, so it clearly corresponds to the final pending constraint set used to
    solve the specialization.
- [x] Ensure the candidate set uses exact `BoundTypeVarIdentity` values, including freshness
    nonces, so freshened generic callable occurrences remain distinct. Generic contexts mentioned by
    assigned types are collected after any call-site freshening, so the candidates match the identities
    that can appear in the specialized return type.

Rationale: for `higher(identity)`, the leaked `A@higher` is a callee inferable typevar. For more
complex cases like `partial(partial, drop)`, leaked typevars can come from generic callable arguments;
those are discovered either via deferred quantification from callable relations or by collecting
freshened generic contexts from assigned specialization types. For a callable parameter typed with an
outer `T`, that outer `T` should not be in the candidate set and therefore should not be rebound.

### Phase 5: Wire call-result rebinding into call binding

- [x] Extend the shared `types/typevar/return_callables.rs` entry point/options to support the new
    call-result use case. The call-result use needs to pass:

    - `db`,
    - the already-specialized call result `Type`, and
    - the candidate `InferableTypeVars` / candidate identity set from Phase 4.

- [x] For the call-result use case:

    - traverse the already-specialized call result as the returned-value root;
    - pass an eligibility callback using the explicit candidate set
        (`inferable ∪ deferred_quantification ∪ generic contexts from assigned types`);
    - do **not** filter by source definition;
    - do **not** remove anything from any callee signature generic context; this is a post-call result
        rewrite only.

- [x] In `ArgumentTypeChecker::infer_specialization` in
    `crates/ty_python_semantic/src/types/call/bind.rs`, capture rescoping candidates immediately
    before `builder.build_with(...)`, then apply the shared helper immediately after:

    ```rust
    self.return_ty = self.return_ty.apply_specialization(self.db, specialization);
    ```

    This should happen inside `infer_specialization`, not later in `Binding::check_types` or
    `finish()`, so `self.return_ty` is final once specialization inference completes.

- [x] Pass the candidate typevar set assembled during specialization inference.

- [x] Use the candidate set as additional inferable typevars for post-specialization argument
    assignability checks. This prevents a valid call like `partial(partial, drop)` from producing a
    false positive against a specialization that still contains generic callable argument typevars
    before returned-callable rescoping.

- [x] Keep non-generic calls and calls with no candidate typevars on the caller-side fast path with
    no rewrite; avoiding unnecessary traversal is the caller's responsibility.

- [x] Review special-case return-type overrides in `Bindings::evaluate_known_cases`; if any special
    cases synthesize callables from generic callable arguments, decide whether they also need the
    rescoping helper or should be left unchanged.

- [x] Guard against over-rebinding class context variables during call-result rescoping: bound
    receiver argument types are passed as outside roots, and class-bound typevars are filtered out of
    the eligibility predicate. Existing ParamSpec mdtests cover a `Container[**P].method` case where
    `P@Container` must remain scoped to the class.

### Phase 6: Update and extend feature tests

- [x] Re-establish the new feature's red baseline if needed by running the mdtests that contain the
    parent revision's red/green cases and noting the exact sections:

    - `Generic callable returned from a higher-order call`
    - `Multiple occurrences of a higher-order generic callable`
    - any related legacy equivalents

- [x] Update the red mdtests in both PEP-695 and legacy callable mdtest files:

    - remove `TODO` markers where the feature is now implemented,
    - remove now-invalid `invalid-argument-type` expectations when rebinding fully fixes the call,
    - update reveal expectations to the desired generic callable and literal return types,
        accounting for the agreed `'return` suffix on rebound typevars.
    - For valid rebinding examples, aim for `TODO: no error`. If false-positive errors remain after
        rebinding, investigate whether they are due to this feature or an existing
        inference/specialization gap; if out of scope and non-trivial, keep the mdtest passing with
        explicit TODO/xfail comments documenting the known limitation.
    - Current status: `higher(identity)` and `partial(partial, drop)` are green in both syntax
        variants.

- [x] Add or update a regression showing that surrounding result correlations are preserved:

    ```py
    def pair[X, Y](f: Callable[[X], Y]) -> tuple[Y, Callable[[X], Y]]: ...
    reveal_type(pair(identity))
    ```

    The existing parent tests already include this; verify it remains unchanged.

- [x] Consider adding a focused regression for an enclosing generic typevar to ensure it is not
    accidentally rebound as callable-local, e.g. a generic outer function whose call result is
    `Callable[..., T@outer]`.

- [x] Add or update focused regressions for nested returned callables, including:

    - `Callable[[], Callable[[T], T]]` binds on the inner callable;
    - `Callable[[Callable[[T], T]], int]` binds on the outer callable because the nested callable is a
        parameter type;
    - identical sibling returned callables can be used independently after rebinding, relying on
        generic-callable freshening rather than occurrence-aware replacement.

    Do not add tests for `Callable[[], tuple[T, Callable[[T], T]]]` in this PR. That shape is
    conceptually useful for explaining ownership, but it is analogous to `def f[T]() -> T: ...` where
    `T` cannot be inferred from a non-callable return value. If diagnostics for that are missing or
    incomplete, that is out of scope.

- [x] Run the two affected mdtest files and review any snapshot updates.

### Phase 7: Broader validation and cleanup

- [x] Run the relevant crate tests if the change touches shared generic/constraint code:

    ```sh
    CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always \
      CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
      cargo test -p ty_python_semantic
    ```

- [x] Run targeted formatting / pre-commit for changed files using the repository's jj-aware hook
    wrapper:

    ```sh
    /home/dcreager/bin/jpk run --files <changed-files>
    ```

- [x] Review `jj diff --git` and `jj diff --stat`.

- [x] Update this plan's status markers before handing off or finishing.

## Open questions / risks

- [x] Candidate-set plumbing: use `inferable ∪ deferred_quantification` from the final
    specialization constraint set, plus generic-context typevars mentioned by assigned specialization
    types. Make `ConstraintSet::deferred_quantification` `pub(crate)` and read it directly instead of
    adding an accessor method.
- [x] Nested callable ownership: bind each candidate on the innermost returned callable that covers
    all eligible occurrences, and apply this same rule to both signature-based and call-result
    rebinding. This means `Callable[[], Callable[[T], T]]` becomes
    `() -> [T'return](T'return) -> T'return`. Conceptually,
    `Callable[[], tuple[T, Callable[[T], T]]]` would bind `T` on the outer callable, but do not make
    that shape a test target in this PR because it is analogous to an uninferable
    `def f[T]() -> T: ...` return-only typevar.
- [x] Ownership frames are only created for callables in returned-value positions. At the root
    returned type, callables discovered anywhere structurally inside containers/unions/intersections
    are returned values and can create frames. Within a callable signature, callables nested in
    parameter types do not bind typevars away from the surrounding returned callable; e.g.
    `Callable[[Callable[[T], T]], int]` is generic at the outer callable level.
- [x] Reuse code from `remove_callable_only_typevars` as much as possible. Move the shared
    implementation to `types/typevar/return_callables.rs`, leave
    `GenericContext::remove_callable_only_typevars` as a shim, and prefer a single exported function
    parameterized by an eligibility filter callback. Renaming rebound typevars with a `'return` suffix
    is preferred because it lets the new call-result rescoping share the existing rescoping machinery
    instead of forking an almost-identical implementation. Consider replacing this with nonce
    freshening only as a separate follow-on change for all returned-callable rescoping logic.
- [x] Candidate set for retrying inference: use the final specialization builder only, not the
    union of any abandoned first attempt and the final attempt.
- [x] Candidate capture timing: capture the final candidate set immediately before
    `builder.build_with(...)`, so it matches the final pending constraint set used to solve.
- [x] Candidate set exactness: use
    `self.inferable_typevars ∪ pending.deferred_quantification ∪ assigned generic-context typevars`,
    not just variables directly in the signature generic context. Guard against over-rebinding
    enclosing class/function correlations with tests.
- [x] Manual callable traversal: the shared helper should not rely on `walk_callable_type` /
    `walk_signature`, because it must distinguish returned-value positions from parameter types and
    should not count signature generic-context metadata as ordinary occurrences. Function types should
    be manually traversed for parameter/return occurrences but should not create frames or be
    rewritten.
- [x] Eligibility filter input: normalize ParamSpec attrs before invoking the callback, so the
    callback receives the base `P` rather than `P.args` or `P.kwargs`, until we have a use case that
    needs the original occurrence.
- [x] Outside roots API: pass outside roots as `Type` values only; the signature-based shim can
    flatten parameter annotations and defaults into `impl IntoIterator<Item = Type<'db>>`. Include
    defaults only because that is the existing behavior; avoid unnecessary churn.
- [x] Helper result shape: return a tuple of the rewritten return type plus an `FxIndexMap` of
    original rebound typevars keyed by `BoundTypeVarIdentity`; keep the callable replacement map as
    an internal implementation detail. A named result struct is unnecessary. This avoids allocating a
    separate returned set. The signature shim can filter with
    `contains_key(&bound_typevar.identity(db))`, removing rebound signature typevars by identity.
- [x] Internal ownership representation: use `BoundTypeVarIdentity` for ownership sets/maps and a
    representative normalized `BoundTypeVarInstance` only for rewriting/generic-context construction.
- [x] Ordering: preserve stable first-occurrence order via the global representative `FxIndexMap`.
    When building per-callable replacement maps, iterate that global map and select identities owned
    by the callable, rather than relying on hash/set or child traversal order.
- [x] Phase ordering: first refactor and validate the existing signature-based rebinding path only;
    add the new inference-based call-result rebinding only after that refactor is green. Each jj
    revision should be self-contained, with tests passing and hooks/lints applied. If a behavior must
    be deferred, document it with TODO/xfail mdtest comments before considering that revision done.
- [x] Shared helper should be a plain function, not `#[salsa::tracked]`; it takes a closure and is
    called from already-cached/tracked paths as appropriate.
- [x] Eligibility callback type: use `Fn(BoundTypeVarInstance<'db>) -> bool` for now; both known
    callers are pure predicates, and this can be relaxed later if needed.
- [x] Fast paths: callers are responsible for skipping the helper when no rebinding is possible
    (`generic_context == None` for signature-based use, `InferableTypeVars::None` for call-result
    use). No separate contains-callable precheck is needed; the traversal is equivalent in cost to
    callable detection.
- [x] Invalid generic calls: apply call-result rebinding whenever `infer_specialization` computes a
    specialized return type and non-empty candidate set, even if later argument checks emit errors.
- [x] False-positive argument errors after rebinding: valid examples should generally aim for
    `TODO: no error`, but investigate remaining errors before expanding scope. The existing
    "Generic callable arguments do not hide real argument errors" stanza is a known caution: solutions
    can include artifacts such as `T = int | str | A@identity`, causing false positives in the
    post-specialization argument check. Punt non-trivial existing gaps with explicit TODO/xfail
    comments.
- [x] Call-result integration point: apply rebinding inside `ArgumentTypeChecker::infer_specialization`
    immediately after applying the specialization to `self.return_ty`, not later in `finish()` or
    `Binding::check_types`.
- [x] Freshness interactions: freshened generic callable occurrences passed as arguments are
    represented in the candidate set with matching identities. `partial(partial, drop)` now rebinds
    `X@drop` and `Y@drop` onto returned callables, and both nested calls infer literal return types.
- [x] Identical sibling callables / freshening: for a type like
    `tuple[Callable[[T], T], Callable[[T], T]]`, the desired behavior is that each returned callable
    occurrence can be used independently, e.g. after `c1, c2 = f(identity)`, `c1(1)` and `c2("x")`
    both succeed. This is a generic-callable freshening concern rather than an occurrence-keyed
    replacement-map concern: both tuple elements may receive the same rebound callable type, and
    later call-site freshening should give each use independent nonces.
- [x] Existing generic returned callables: if a callable is already generic, do not create an
    ownership frame for it and do not rewrite it. Treat a callable as already generic if any overload
    signature has a `generic_context`. Still traverse its parameter and return types to discover
    candidate occurrences, attributing them to the current enclosing frame. Do not add an extra filter
    for the callable's own generic-context variables unless a concrete use case requires it.
- [x] Callable-signature defaults: do not traverse parameter default types inside callable
    signatures, matching existing `walk_signature` behavior. Continue including top-level function
    parameter defaults as outside roots only because that is existing behavior.
- [x] Type alias traversal: keep broad lazy-attribute traversal disabled, but preserve the existing
    special-case recursion into PEP 695 and manual PEP 695 type aliases in both outside and
    returned-value roots.
- [x] Alias-preserving rewrite: use the existing `TypeMapping::RescopeReturnCallables` rewrite
    rather than manually rebuilding the full return type, so alias/rewrite behavior stays unchanged.
- [x] Callable replacement map collection: keep `TypeMapping::RescopeReturnCallables` using
    `FxHashMap<CallableType, CallableType>` unless implementation friction suggests otherwise; stable
    order matters for typevar replacement maps, not callable replacement lookup.
- [x] Typevar occurrence traversal: match existing behavior by recording the occurrence itself only;
    do not recurse into bounds, constraints, or defaults attached to the typevar.
- [x] ParamSpec cases may require additional tests if returned callable signatures involving
    `Callable[P, R]` regress. The existing PEP-695 ParamSpec mdtest caught an over-rebinding
    regression for class-scoped ParamSpecs, and now passes.
