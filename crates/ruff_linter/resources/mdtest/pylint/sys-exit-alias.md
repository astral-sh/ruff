# `sys-exit-alias` (`PLR1722`)

```toml
[lint]
select = ["PLR1722"]
```

See <https://github.com/astral-sh/ruff/issues/4419> for the fix behavior around conditionally
bound imports.

## Conditionally bound import

`sys` is only bound if the branch executes, so no fix is offered.

```py
if False:
    import sys

exit(1)  # snapshot: sys-exit-alias
```

```snapshot
error[PLR1722]: Use `sys.exit()` instead of `exit`
 --> src/mdtest_snippet.py:4:1
  |
4 | exit(1)  # snapshot: sys-exit-alias
  | ^^^^
  |
help: Replace `exit` with `sys.exit()`
```

## Import in the same branch

The import and the call are in the same branch, so the fix can use it.

```py
if cond:
    import sys

    exit(1)  # snapshot: sys-exit-alias
```

```snapshot
error[PLR1722]: Use `sys.exit()` instead of `exit`
 --> src/mdtest_snippet.py:4:5
  |
4 |     exit(1)  # snapshot: sys-exit-alias
  |     ^^^^
  |
help: Replace `exit` with `sys.exit()`
  |
3 |
  -     exit(1)  # snapshot: sys-exit-alias
4 +     sys.exit(1)  # snapshot: sys-exit-alias
  |
note: This is an unsafe fix and may change runtime behavior
```

## Unconditional import

The import is unconditional, so the fix can use it inside a branch.

```py
import sys

if cond:
    exit(1)  # snapshot: sys-exit-alias
```

```snapshot
error[PLR1722]: Use `sys.exit()` instead of `exit`
 --> src/mdtest_snippet.py:4:5
  |
4 |     exit(1)  # snapshot: sys-exit-alias
  |     ^^^^
  |
help: Replace `exit` with `sys.exit()`
  |
3 | if cond:
  -     exit(1)  # snapshot: sys-exit-alias
4 +     sys.exit(1)  # snapshot: sys-exit-alias
  |
note: This is an unsafe fix and may change runtime behavior
```
