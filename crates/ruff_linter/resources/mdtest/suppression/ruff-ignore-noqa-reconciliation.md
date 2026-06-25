# Reconciling `noqa` and `ruff:ignore` behavior

Regression tests for <https://github.com/astral-sh/ruff/issues/26282>.

## F401: `ruff:ignore` protects imports from autofixes

Shared import fixes for F401 should respect `ruff:ignore` suppressions, the same way they respect
`noqa` directives. Imports protected by either mechanism should not be removed by the autofix.

```toml
[lint]
preview = true
select = ["F401"]
```

```py
from package import (
    kept,  # noqa: F401
    # error: [unused-import]
    removed,
)

from package2 import (
    kept2,  # ruff:ignore[F401]
    # error: [unused-import]
    removed2,
)
```

## RUF100 self-suppression via `ruff:ignore`

`ruff:ignore[RUF100]` should not be reported as an unused suppression by RUF100 itself, matching
the existing `noqa`-side self-suppression behavior. The check in `check_suppressions` skips
suppressions whose resolved code is RUF100.

```toml
[lint]
preview = true
select = ["RUF100"]
```

On the `noqa` side, self-suppression already works:

```py
x = 1  # noqa: RUF100
```

On the suppression side, `ruff:ignore[RUF100]` should also be silently accepted:

```py
x = 1  # ruff:ignore[RUF100]
```

## RUF101: Redirected codes in `ruff:ignore` comments

The redirected-noqa check (RUF101) should also inspect `ruff:ignore` suppression comments for
redirected codes, not just `noqa` directives. `U007` is a redirect to `UP007`.

```toml
[lint]
preview = true
select = ["RUF101"]
```

### `noqa` baseline

```py
# error: [redirected-noqa]
x = 2  # noqa: U007
```

### `ruff:ignore` with redirected code

This is the new behavior from the fix: `ruff:ignore` comments are also checked for redirects.

```py
# error: [redirected-noqa]
x = 2  # ruff:ignore[U007]
```

## Redirect alias RUF100 bookkeeping

When a suppression matches a diagnostic, redirect aliases (different original codes that resolve to
the same target) should also be marked as used, preventing false "unused" reports.

`PGH001` is a redirect to `S307`.

```toml
[lint]
preview = true
select = ["S307", "RUF100"]
```

### Suppression side: both codes on the same `ruff:ignore` line

When `S307` matches the diagnostic and `PGH001` (which redirects to `S307`) is also present,
`PGH001` should be marked as used:

```py
eval("1+1")  # ruff:ignore[S307, PGH001]
```

### Noqa side: redirect alias not reported as unused

When `# noqa: PGH001` suppresss an `S307` diagnostic, the noqa should not be reported as unused
even though `PGH001` doesn't match `S307` literally:

```py
eval("1+1")  # noqa: PGH001
```
