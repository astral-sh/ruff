## What it does

Checks for invalid match patterns.

## Why is this bad?

Invalid match patterns will cause a `TypeError` at runtime. Specifically:

- Using a non-type object in a class pattern.
- Providing more positional sub-patterns than `__match_args__` allows.
- Matching against `collections.abc.Callable` with positional sub-patterns.
- Matching against a non-runtime-checkable protocol.
- Matching against a `TypedDict`.

## Examples

```python
class Point:
    __match_args__ = ("x", "y")


def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):  # error: [invalid-match-pattern]
            ...
```
