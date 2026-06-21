## What it does

Checks that `match`/`case` class patterns are consistent with a class's
`__match_args__` attribute.

## Why is this bad?

An inconsistency between a class pattern and `__match_args__` will cause a
`TypeError` at runtime. Specifically:

- Providing more positional sub-patterns than `__match_args__` has entries
    raises `TypeError` at runtime.

## Examples

```python
class Point:
    __match_args__ = ("x", "y")
    x: int
    y: int


def describe(p: Point) -> None:
    match p:
        case Point(x, y, z):  # error: [bad-match]
            ...
```
