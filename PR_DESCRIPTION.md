[`pyflakes`] Fix F821 false positive for class-scope generator expressions (#23364)

## Summary

Fixes #23364.

`F821` (`undefined-name`) fires on a class name used inside a generator
expression at class scope:

```python
class Foo:
    BAR = [1, 2, 3]
    BAZ = [4, 5, 6]
    QUX = ((first, second) for first in BAR for second in Foo.BAZ)
    #                                                   ^^^ F821
```

But the name resolves at runtime: a generator body and its non-first
iterables run lazily, when the generator is consumed, after the
enclosing class is fully defined.  The checker visited those parts
eagerly during the class body traversal, before the class name existed
in the enclosing scope.

## Fix

Defer class-scope generator expressions — the same way lambdas are
already deferred — so that forward references resolve against bindings
that exist after the class is defined.

In `visit_expr` for `Expr::Generator`, when an ancestor scope is a
class scope:

1. Visit the first iterable's iterator expression in the enclosing
   scope eagerly (matching CPython's evaluation order: the first
   iterable runs in the class body).
2. Push a `ScopeKind::Generator` scope and save a snapshot onto
   `Visit::generators`.

In `visit_deferred_generators()` (new method, called after
`visit_deferred_lambdas` in `visit_deferred`):

1. Restore the snapshot (already inside the Generator scope).
2. Replay the remaining `visit_generators` logic: first generator's
   target and ifs, plus all subsequent generators' iter/target/ifs.
3. Visit the generator body expression (`elt`).

Module- and function-scope generators stay eager to avoid ecosystem
regressions with `except ... as e` handler variables and names removed
by `del` (those bindings are gone by the time the deferred pass runs).

## Trade-offs

- **False negative**: the first iterable is also deferred, so
  references to the class name in the first iterable are not flagged
  (they would fail at runtime with `NameError`).  Ruff prefers false
  negatives over false positives in this trade-off.
- List, set, and dict comprehensions are unchanged — they run eagerly
  and correctly flag class-name references.

## Test Plan

`cargo test -p ruff_linter` — all 2799 tests pass.

New fixture `F821_35.py` covers:
- Class name in non-first generator iterable (was F821, now fixed)
- Class name in generator body (was F821, now fixed)
- Nested generators (was F821, now fixed)
- List/set/dict comprehensions still flag correctly
- Module-level and function-scope generators unchanged
