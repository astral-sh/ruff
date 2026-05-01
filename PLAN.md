# PLAN: Deterministic bound-typevar freshening

## Repository / handoff instructions

- This repository normally uses Jujutsu (`jj`). Do not make code changes without creating/using a dedicated jj revision via `jj new` (usually `jj new -A @`). Do not use `jj edit` to modify an existing revision. Prefix agent-authored revision descriptions with `[π]`.
- Treat this file as the handoff source of truth. When resuming, first validate the status markers against the code and update this file if reality has changed.
- Use `$HOME/.pi/tmp` for temporary files, not `/tmp`.
- For final validation in a jj worktree, run `/home/dcreager/bin/jpk run -a` (not `uvx prek`).
- For ty mdtests, prefer the repo command style from `AGENTS.md`, e.g.
    `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::<path>`.

## Current status

- [x] Bound typevars already have occurrence freshness plumbed through `BoundTypeVarInstance` / `BoundTypeVarIdentity`.
- [x] Direct call binding has an exploratory `TypeVarNonceGenerator` that can seed enclosing generic contexts and freshen later occurrences.
- [x] The brittle `is_library_typevar` guard was removed during discussion.
- [x] The implicit relation-side seeding based on `given.mentions_any_typevar` was removed during discussion.
- [x] The helper `ConstraintSet::mentions_any_typevar` was removed after it became unused.
- [x] Confirmed that strict "seed only enclosing contexts + freshen on second/later occurrence" still times out on the scikit-learn/NumPy repro.
- [x] Debugged the remaining timeout: relation freshening repeatedly revisits the same NumPy `ndarray` dunder overload signatures against the same scikit-learn `Array` protocol methods. These appear to be repeated proof-search visits, not genuine sibling/recursive callable occurrences.
- [x] Relation-side signature freshening has been re-enabled with deterministic collision-avoidance freshening in `crates/ty_python_semantic/src/types/signatures.rs`. The temporary `let next_freshening_nonce = |_signature: &Signature<'db>| None;` exploratory state has been removed.
- [x] `ConstraintSet.implies_subtype_of` now freshens generic contexts mentioned in its lhs/rhs types before running the relation check, so the generic-callable expectations in `type_properties/implies_subtype_of.md` pass without TODO error annotations.

## Problem statement

The original occurrence-count approach for relation-side freshening used a generator-wide `seen: FxHashSet<GenericContext>`:

- First occurrence of a generic context: do not freshen.
- Second/later occurrence in the same generator lifetime: freshen with a new nonce.

This works for direct calls with sibling callable occurrences, but it is the wrong abstraction for signature relation checks. A relation traversal can inspect the same source/target signature pair many times while proving protocol conformance or overload relations. Counting visits makes each repeated inspection allocate a new nonce, which creates distinct freshened signatures and can prevent recursion/proof-search machinery from recognizing that work is being repeated.

The scikit-learn repro:

```py
import numpy as np
from sklearn.utils._array_api import xpx

data = np.array([1.0])
data = xpx.nan_to_num(data)
```

With strict occurrence-count relation freshening, this times out after 10s. Debug output showed thousands of repeated freshenings of NumPy stub generic contexts such as `_NumberT`, `_RealNumberT`, `_NumericArrayT`, `_ShapeT`, and `_IntegralArrayT`, mostly from `numpy/__init__.pyi` `ndarray` dunder methods compared against `sklearn/externals/array_api_extra/_lib/_utils/_typing.pyi` `Array` protocol methods.

A temporary library-path guard fixed the perf issue but was brittle and masked the underlying problem.

## Revised design hypothesis

Relation-side freshening should be deterministic alpha-renaming by collision avoidance, not occurrence counting.

When comparing signatures, the goal is to ensure that locally-bound callable typevars from one signature do not accidentally collide with typevars visible in the other signature. If there is no collision, do not freshen. If there is a collision, bump the freshness nonce deterministically by enough to make the local binder disjoint.

This makes repeated visits to the same signature pair produce the same result instead of allocating nonce #1, #2, #3, ... each time.

### Proposed nonce representation change

Change bound-typevar freshness from `Option<NonZeroU32>` to a plain `u32` value:

- `0` means "not freshened".
- Positive values are freshness nonces.
- Plain `u32` makes deterministic bump arithmetic easier: `new_nonce = old_nonce + delta`.

The display/debug cleanup for freshened typevars can remain follow-up work unless tests require updates.

### Proposed deterministic generic-context bump

Freshening is decided per generic context, not per individual typevar.

For a generic context `G` on one side of a signature relation:

1. Collect the base identities of the typevars bound by `G` (freshness normalized to `0`).

1. Scan the *entire other signature* for occurrences of any of those base identities. The other signature's own generic context is not special; ambient/free typevars in the other signature can also collide with `G`.

1. If no matching base identities occur in the other signature, leave `G` unchanged.

1. Otherwise, compute one shared delta for the whole context:

    ```text
    delta = max_nonce_seen_for_any_matching_base_identity + 1
    ```

1. Bump every typevar bound by `G` by that same `delta`.

This intentionally treats seeing `Tn` on the other side as reserving the whole range `T0..Tn`. For example, if the other signature contains `T2`, then a colliding context is bumped by `3`, even if that context's own `T` was already freshened to `T1`.

Only typevars bound by the side's own generic context are renameable. Free/free overlap remains meaningful and must not be freshened away.

Base identity does not need a separate representation. Add helper methods on `BoundTypeVarIdentity` / `BoundTypeVarInstance` that compare identities while ignoring freshness and expose the current freshness nonce. This is more intuitive than materializing a normalized `nonce = 0` identity, and still preserves the true binding identity (source `TypeVarIdentity`, binding context, paramspec attr, etc.).

## Important semantic distinction

The relation check itself should only handle disjointness between the signatures being compared.

Ambient constraints used by `ConstraintSet.implies_subtype_of(...)` are a separate layer. For example:

```py
def identity2[T](x: T) -> T:
    constraints = ConstraintSet.range(bool, T, int)
    static_assert(constraints.implies_subtype_of(TypeOf[identity2], Callable[[str], str]))
```

The relation `TypeOf[identity2] <: Callable[[str], str]` does not see the ambient `constraints`; it should produce constraints involving the callable-local `T`. The collision between the ambient `T` in `constraints` and the relation-local `T` should be handled where implication combines/compares the left-hand constraint set with the right-hand relation result, not by making signature relation freshening scan `given` constraints.

## Open questions / design points to settle

- [x] **Which side(s) can be renamed in signature relation?**

    - Only a side's own generic-context binders are renameable.
    - The collision check must run in both directions: lhs generic context against the full rhs signature, and rhs generic context against the full lhs signature.
    - Example: if `lhs = TypeOf[f]` is generic and binds `T`, while `rhs = Callable[[T], str]` contains an ambient/free `T`, then lhs must be freshened because lhs's generic-context `T` appears somewhere in rhs. The ambient/free rhs `T` must not be renamed.

- [x] **Exactly which typevars participate in relation freshening?**

    - For a side's generic context `G`, collect only the base identities of typevars bound by `G`.
    - Scan the entire other signature for occurrences of those base identities; do not restrict the scan to the other side's generic context.
    - If there is a collision, bump all typevars bound by `G` by one shared delta. Do not bump unrelated typevars, and do not bump free typevars that are not in the side's generic context.
    - Free/free overlap remains meaningful and must not be freshened away.

- [x] **Cheap same-context optimization.**

    - If both signatures have exactly the same `GenericContext`, they definitely collide. Freshen only rhs and skip an unnecessary lhs bump.
    - This is only an optimization. It is fine if exact `GenericContext` equality misses base-equivalent already-freshened contexts; the normal directional scans still produce a correct result, possibly with an unnecessary extra bump.

- [x] **How to represent "base identity"?**

    - No separate representation is needed.
    - Add helper methods on `BoundTypeVarIdentity` / `BoundTypeVarInstance` to compare identities while ignoring freshness and to read the current freshness nonce.
    - `P`, `P.args`, and `P.kwargs` family consistency remains follow-up because ParamSpec freshening is skipped for now.

- [x] **ParamSpec support.**

    - Continue the existing behavior of skipping ParamSpec freshening.
    - `P`, `P.args`, and `P.kwargs` need family-consistent treatment, so full ParamSpec freshness remains follow-up work.

- [x] **Where exactly to freshen for `implies_subtype_of`.**

    - Freshen only the explicit lhs/rhs types passed to the low-level `ConstraintSet.implies_subtype_of` mdtest helper before invoking the relation check.
    - Do not make `SubtypingAssuming` or relation-side signature freshening scan ambient `given` constraints.
    - This keeps implication-layer occurrence freshening separate from deterministic signature-relation collision avoidance.

- [x] **Direct call freshening.**

    - Direct calls still use occurrence-count freshening, but the nonce generator now separates recording/decision from nonce allocation: `record_enclosing_scopes`, `should_freshen`, and `next`.
    - `should_freshen` records unseeded generic contexts as they are encountered, so a second occurrence of an initially unseeded context is freshened correctly.
    - Call-site freshening applies one shared nonce/delta to all overloads that need freshening without first collecting an intermediate nonce vector.
    - This remains compatible with the deterministic freshening operation shape (`generic_context + delta`).

- [ ] **Caching after deterministic freshening.**

    - Deterministic bumping should stop unbounded nonce allocation, but NumPy/protocol relation work may still be repeated.
    - After deterministic freshening is implemented, rerun the scikit-learn repro. If repeated work remains, consider caching the freshening step or the signature-pair relation result with stable keys, e.g. source definition, target definition, relation kind, and stable freshening decision.

## Implementation plan

### Phase 0: Restore a known exploratory baseline

- [x] Started from an exploratory state with relation freshening disabled and no library guard.
- [x] Before implementation, inspect `jj diff --git` and this file. Decided to build on the current exploratory diff in a follow-up jj revision.
- [x] Ensure no debug `eprintln!` statements remain before running tests.

### Phase 1: Convert freshness representation to `u32`

- [x] Change `TypeVarNonce` / bound-typevar freshness storage from `Option<NonZeroU32>` to a `u32`-backed representation.
- [x] Preserve `0 == not freshened` semantics.
- [x] Add helpers for:
    - current freshness value,
    - base identity with freshness zeroed,
    - adding a deterministic delta.
- [x] Run `cargo check -p ty_python_semantic`.
    - Passed in revision `[π] Convert typevar freshness nonce representation`.

### Phase 2: Add typevar freshness collection utilities

- [x] Add helpers to compare typevars against the identities bound by a `GenericContext` while ignoring freshness.
- [x] Add traversal helpers to scan a `Type` / `Signature` for occurrences that match a caller-provided generic context ignoring freshness, returning the maximum freshness nonce seen.
- [x] Add analogous traversal support for `ConstraintSet` / `OwnedConstraintSet` if needed for implication-layer work.
    - Not needed for now; `implies_subtype_of` freshening remains punted.
- [x] Run `cargo fmt` and `cargo check -p ty_python_semantic` after Phase 2 changes.

### Phase 3: Add generic-context bumping

- [x] Add an `ApplyTypeMapping` variant or dedicated visitor that bumps all typevars bound by a specific `GenericContext` by one shared `delta`.
    - Recast existing `TypeMapping::FreshenBoundTypeVars` to store a `delta`; added `Signature::freshen_bound_typevars`.
- [x] Ensure it updates the signature's `generic_context` consistently with parameter and return types.
- [x] Keep ParamSpecs unchanged initially, unless explicitly supported.
    - The delta mapping skips freshening ParamSpec typevars themselves, but still freshens non-ParamSpec typevars in the same generic context.
- [x] Run `cargo fmt` and `cargo check -p ty_python_semantic` after Phase 3 changes.

### Phase 4: Replace relation-side occurrence-count freshening

- [x] Remove relation-side dependency on `TypeVarNonceGenerator::seen`.
- [x] In `check_signature_pair`, deterministically freshen each side's generic context by scanning the full opposite signature for colliding base identities.
- [x] If both sides have exactly the same `GenericContext`, freshen rhs only as a cheap optimization.
    - Implemented as two simple checks: freshen source only when its generic context differs from target's; freshen target whenever it collides.
- [x] Preserve free/free typevar sharing.
- [x] Re-enable relation freshening; remove the temporary `None` closure.
- [x] Run focused mdtests:
    - `mdtest::type_properties/implies_subtype_of.md`
    - `mdtest::generics/pep695/functions.md`
    - `mdtest::generics/legacy/functions.md`
    - `mdtest::generics/pep695/callables.md`
    - `mdtest::generics/legacy/callables.md`
    - Result: generics mdtests passed; `type_properties/implies_subtype_of.md` still has expected generic-callable failures at lines 525 and 568. These are the implication-layer cases deferred to Phase 5.
- [x] Run `cargo fmt` and `cargo check -p ty_python_semantic` after Phase 4 changes.

### Phase 5: Update mdtests for explicit implication-layer freshening

- [x] Implement implication-layer freshening only for the low-level `ConstraintSet.implies_subtype_of` helper by freshening generic contexts mentioned in the explicit lhs/rhs types before the relation check.
- [x] Review mdtests that use `ConstraintSet.implies_subtype_of` with generic callables.
- [x] Freshen the low-level helper's lhs/rhs types directly, so callable-local typevars are already distinct before relation reduction and same-identity ambient constraints are preserved.
- [x] Re-run focused mdtests.
    - Passed:
        `mdtest::type_properties/implies_subtype_of.md`,
        `mdtest::generics/pep695/functions.md`,
        `mdtest::generics/legacy/functions.md`,
        `mdtest::generics/pep695/callables.md`,
        `mdtest::generics/legacy/callables.md`.
    - `cargo check -p ty_python_semantic` passed after the helper change.

### Phase 6: Validate scikit-learn performance

- [x] Build profiling `ty`.

    - `cargo build --profile profiling --bin ty` passed.

- [x] Run the minimal repro from a scikit-learn checkout with its venv:

    ```py
    import numpy as np
    from sklearn.utils._array_api import xpx

    data = np.array([1.0])
    data = xpx.nan_to_num(data)
    ```

    - Set up checkout at `$HOME/.pi/tmp/scikit-learn-repro` using `uv run scripts/setup_primer_project.py scikit-learn ...`.
    - Ran from the checkout root: `target/profiling/ty check --output-format=concise pi_repro.py`.

- [x] Confirm it no longer times out.

    - Minimal repro passed in ~0.59s wall time (`All checks passed!`).

- [x] Run `eco -o $HOME/.pi/tmp/scikit-learn-ecosystem-<name>.json scikit-learn` and compare timing/diagnostics.

    - Feature run: `$HOME/.pi/tmp/scikit-learn-ecosystem-phase6.json`, `time_s = 1.4254865646362305`, diagnostics = 1978, return code = 1.
    - Main run: `$HOME/.pi/tmp/scikit-learn-ecosystem-main.json`, `time_s = 1.3866698741912842`, diagnostics = 1978, return code = 1.
    - Diagnostic JSON comparison found 0 added and 0 removed diagnostics vs main.

### Phase 7: Broader validation and cleanup

- [x] Run all ty mdtests: `cargo nextest run -p ty_python_semantic -- mdtest::` with the usual env vars.
    - Passed: 334 mdtests passed, 249 skipped.
- [x] Review any changed snapshots.
    - No snapshot files were changed by the mdtest run.
- [x] Run `/home/dcreager/bin/jpk run -a`.
    - Passed.
- [x] Update this `PLAN.md` with final status and any remaining follow-up work.

## Test cases to add or verify

- [x] Recursive generic callable direct-call examples already added earlier should still pass.
    - Covered by all ty mdtests passing.
- [x] Sibling generic callable occurrences in one direct call, e.g. passing `identity` multiple times to a higher-order function, should still freshen independently.
    - Covered by focused generics callable/function mdtests and all ty mdtests passing.
- [x] Generic callable compared to concrete callable under unrelated ambient constraints should pass.
    - Covered by `type_properties/implies_subtype_of.md` and all ty mdtests passing.
- [x] Generic callable compared to `Callable[[T], T]` where `T` is an ambient/free typevar should preserve the ambient `T` and freshen only the callable-local binder if needed.
    - Covered by `type_properties/implies_subtype_of.md` and all ty mdtests passing.
- [x] Free/free typevar sharing between two non-generic callables should remain correlated and must not be freshened away.
    - Covered by focused generics callable/function mdtests and all ty mdtests passing.
- [x] A minimized NumPy/protocol-style case should not repeatedly allocate new freshness values for the same signature pair.
    - Minimal scikit-learn/NumPy repro passed in ~0.59s; full scikit-learn ecosystem run completed in ~1.43s with no diagnostic changes vs main.

## Known commands / observations

Focused mdtests command used earlier:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 \
  cargo nextest run -p ty_python_semantic -- \
  mdtest::type_properties/implies_subtype_of.md \
  mdtest::generics/pep695/functions.md \
  mdtest::generics/legacy/functions.md \
  mdtest::generics/pep695/callables.md \
  mdtest::generics/legacy/callables.md
```

Strict occurrence-count relation freshening result:

- `cargo check -p ty_python_semantic`: passed.
- Minimal scikit-learn repro: timed out after 10s.

Relation freshening disabled result:

- Minimal scikit-learn repro: avoids the freshening blowup.
- Focused mdtests: `type_properties/implies_subtype_of.md` fails on generic callable assertions.
