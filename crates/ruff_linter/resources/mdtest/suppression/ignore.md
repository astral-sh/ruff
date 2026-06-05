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

## Empty diagnostic at end of file

Diagnostics with empty ranges should also be suppressible, as with `noqa`.

```toml
[lint]
preview = true
select = ["W292"]
```

```py
suppressed = 1  # ruff:ignore[W292]
```

## Block suppression boundaries

A block suppression should not apply to a diagnostic that starts inside the disabled range but ends
after the matching `ruff:enable` comment:

```toml
[lint]
preview = true
select = ["RUF015"]
```

```py
# ruff:disable[RUF015]
# error: [unnecessary-iterable-allocation-for-first-element]
not_suppressed = [
# ruff:enable[RUF015]
    *range(10)
][0]
```

This isn't _strictly_ a problem because the formatter will indent and invalidate the `ruff:enable`
comment here, raising `RUF103`, but it seems better for this not to work regardless of formatting.

This is how the comments should look instead:

```py
# ruff:disable[RUF015]
not_suppressed = [
    *range(10)
][0]
# ruff:enable[RUF015]
```

## Make sure the code actually matches

```toml
[lint]
preview = true
select = ["RUF015"]
```

```py
# error: [unnecessary-iterable-allocation-for-first-element]
not_suppressed = [  # ruff:ignore[F401]
    *range(10)
][0]
```
