## What it does

Checks for statically invalid match patterns.

## Why is this bad?

Invalid patterns can raise an error at runtime. For class patterns, ty checks the statically known
type of `__match_args__`.

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
