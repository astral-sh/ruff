# Tests for `ruff:ignore` comments

## End-of-line `ruff:ignore` range

This is a regression test for <https://github.com/astral-sh/ruff/issues/25644>, where `ruff:ignore`
comments behaved differently from `noqa` and `ty:ignore` comments when placed on the first line of a
diagnostic range.

```toml
[lint]
preview = true
select = ["RUF015"]
```

```py
suppressed = [  # noqa: RUF015
    *range(10)
][0]

not_suppressed = [  # ruff:ignore[RUF015]
    *range(10)
][0]
```
