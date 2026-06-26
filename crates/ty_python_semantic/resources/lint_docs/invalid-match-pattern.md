## What it does

Checks for invalid match patterns.

## Why is this bad?

Matching on invalid patterns will lead to a runtime error.

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
    # TypeError at runtime: must be a class
    case NotAClass():  # error
        ...
```
