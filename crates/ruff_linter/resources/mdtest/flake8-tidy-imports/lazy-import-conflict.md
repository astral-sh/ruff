# `lazy-import-mismatch` (`TID254`) and `lazy-import-immediately-resolved` (`TID255`) conflict

Regression tests for the infinite fix loop between `lazy-import-mismatch` (TID254) and
`lazy-import-immediately-resolved` (TID255), reported in
<https://github.com/astral-sh/ruff/issues/25418>.

When `require-lazy` forces an import to be lazy but that import is resolved immediately at module
load time, the two rules' fixes used to undo each other: TID254 inserts `lazy`, TID255 strips it,
and so on forever. TID255 now defers to TID254 for any import that `require-lazy` requires to be
lazy, so the import stays lazy and the fixes converge.

## Both rules enabled: required-lazy import resolved immediately

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]

[lint.flake8-tidy-imports]
require-lazy = "all"
```

The eager import matches `require-lazy = "all"`, so TID254 flags it and inserts `lazy`. Even though
`foo` is then used immediately in the class base, TID255 stays silent because the import is required
to be lazy. The fix converges to a single lazy import instead of oscillating (the test harness
panics if a fix fails to converge, so reaching this assertion proves the loop is gone).

```py
import foo  # error: [lazy-import-mismatch]


class C(foo.Base): ...
```

Starting from the already-lazy form is also stable: TID254 is satisfied and TID255 is suppressed, so
nothing fires.

```py
lazy import foo


class D(foo.Base): ...
```

## Member-name `require-lazy` selector on `from ... import ...`

This is the previously-broken case: `require-lazy` targets a specific member (`pkg.thing`), not a
module. TID254 matches that member per-alias and inserts `lazy`; the suppression guard must match
the same member, otherwise TID255 strips `lazy` again and the fixes oscillate forever (the harness
panics on non-convergence, so reaching this assertion proves the loop is gone).

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]

[lint.flake8-tidy-imports]
require-lazy = ["pkg.thing"]
```

```py
from pkg import thing  # error: [lazy-import-mismatch]


class C(thing.Base): ...
```

Starting from the already-lazy form is stable: TID254 is satisfied and TID255 is suppressed.

```py
lazy from pkg import thing


class D(thing.Base): ...
```

## Aliased member import under a member-name `require-lazy` selector

`from x import y as z` selected by `require-lazy = ["x.y"]` (the selector matches the original
member name, not the alias). TID254 inserts `lazy`; the guard must recognize the same aliased
member so the fixes converge instead of oscillating.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254", "TID255"]

[lint.flake8-tidy-imports]
require-lazy = ["x.y"]
```

```py
from x import y as z  # error: [lazy-import-mismatch]


class C(z.Base): ...
```

The already-lazy form is stable.

```py
lazy from x import y as z


class D(z.Base): ...
```

## TID255 still fires when the import is not required to be lazy

Without `require-lazy`, there is no conflict: a lazy import resolved immediately at module load time
is genuinely pointless, so TID255 fires and its fix removes `lazy`.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID255"]
```

```py
lazy import foo


class C(foo.Base): ...  # snapshot: lazy-import-immediately-resolved
```

```snapshot
error[TID255]: Lazy import `foo` is resolved immediately
 --> src/mdtest_snippet.py:4:9
  |
4 | class C(foo.Base): ...  # snapshot: lazy-import-immediately-resolved
  |         ^^^
  |
help: Convert to an eager import
  - lazy import foo
1 + import foo
2 |
3 |
4 | class C(foo.Base): ...  # snapshot: lazy-import-immediately-resolved
note: This is an unsafe fix and may change runtime behavior
```

A lazy import that is *not* resolved immediately (only used inside a function) is fine and is left
alone.

```py
lazy import bar


def use() -> None:
    print(bar.value)
```

## TID254 still fires on an eager import that is not resolved immediately

With only TID254 enabled, an eager import that should be lazy is flagged and made lazy regardless of
where it is used, since there is no TID255 to conflict with.

```toml
target-version = "py315"

[lint]
preview = true
select = ["TID254"]

[lint.flake8-tidy-imports]
require-lazy = "all"
```

```py
import foo  # error: [lazy-import-mismatch]


def use() -> None:
    print(foo.value)
```
