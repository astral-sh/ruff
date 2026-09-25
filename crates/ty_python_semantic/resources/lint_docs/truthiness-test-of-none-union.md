## What it does

Detects truthiness checks of unions with `None` whose non-`None` part can also be falsy, like an
`if value: ...` check on a value of type `int | None`.

This rule is disabled by default. It can catch mistakes where a valid value is confused with missing
data, but may also flag code that deliberately treats `None` and other falsy values alike.

## Why is this bad?

`None` is a sentinel often used to indicate the absence of a meaningful value. A variable annotated
as `int | None` can hold an integer or `None`. Since `None` is falsy, testing an object's truthiness
is a common way to check whether a value is present, but this can lead to incorrect behavior. An
object of type `int | None` can still be falsy even if it is an integer, since `0` is falsy. Boolean
tests on objects of type `str | None`, `bytes | None`, or `list | None` have similar pitfalls: an
empty string, bytestring, or list is also falsy in Python.

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

Checking that `presets` is a list before testing its truthiness preserves the intended behavior
without triggering the rule:

```py
def default_preset(presets: list[str] | None) -> str:
    if isinstance(presets, list) and presets:
        return presets[0]
    return "auto"
```

## See also

`redundant-condition` and `redundant-condition-strict` detect Boolean conditions that are always
true or always false.
