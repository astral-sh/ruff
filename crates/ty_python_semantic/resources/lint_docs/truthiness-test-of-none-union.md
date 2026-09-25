## What it does

Detects truthiness checks of optional values whose non-`None` part can also be false, like an
`if value: ...` check on a value of type `int | None`.

This rule is disabled by default. It can catch mistakes where a valid value is confused with missing
data, but may also flag code that deliberately treats `None` and other falsy values alike.

## Why is this bad?

Types like `int | None` or `str | None` are often used to represent values that may or may not be
present. A Boolean condition such as `if value: ...` does not distinguish `None` from other falsy
values: integers are falsy when they are zero, strings are falsy when they are empty, and containers
are falsy when they have no elements. If the intent of the code was to handle those cases
differently from `None`, a truthiness check can lead to incorrect behavior.

## Examples

Consider a function that returns at most a given number of items, or all items if no limit was
provided:

```py
def take(items: list[str], limit: int | None = None) -> list[str]:
    if not limit:  # error: [truthiness-test-of-none-union]
        return items
    return items[:limit]
```

The intention here is that `take(items, 0)` should return no items. But because both `None` and `0`
are falsy, it returns all of them instead. Checking explicitly for `None` fixes the mistake:

```py
def take(items: list[str], limit: int | None = None) -> list[str]:
    if limit is None:
        return items
    return items[:limit]
```

## Known issues and workarounds

This rule can trigger on conditions that intentionally treat missing and empty values alike. For
example, the following function intentionally treats both `None` and empty lists as equivalent:

```py
def default_preset(presets: list[str] | None) -> str:
    if presets:  # error: [truthiness-test-of-none-union]
        return presets[0]
    return "auto"
```

Replacing the condition with `presets is not None` would introduce an `IndexError` for an empty
list.

An explicit `bool()` call preserves the intended truthiness check without triggering the rule:

```py
def default_preset(presets: list[str] | None) -> str:
    if bool(presets):
        return presets[0]
    return "auto"
```

## See also

`redundant-condition` and `redundant-condition-strict` detect Boolean conditions that are always
true or always false.
