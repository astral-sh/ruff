## What it does

Checks for invalid match patterns.

## Why is this bad?

Invalid match patterns can cause a `TypeError` at runtime. This includes:

- Using a non-type object in a class pattern.
- Providing positional subpatterns when `__match_args__` is missing or has an invalid static type.
- Matching against `collections.abc.Callable` with positional subpatterns.
- Matching against a non-runtime-checkable protocol.
- Matching against a `TypedDict`.

## Examples

```python
class Point:
    __match_args__ = ("x", "y")


def describe(p: Point) -> None:
    match p:
        # TypeError at runtime: Point() accepts 2 positional sub-patterns (3 given)
        case Point(x, y, z):  # error: [invalid-match-pattern]
            ...
```

```python
NotAClass = 42

match object():
    # TypeError at runtime: called match pattern must be a class
    case NotAClass():  # error: [invalid-match-pattern]
        ...
```
