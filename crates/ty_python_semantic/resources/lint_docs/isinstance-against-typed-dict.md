## What it does

Reports runtime checks against `TypedDict` classes.
This includes explicit calls to `isinstance()`/`issubclass()` and implicit
checks performed by `match` class patterns.

## Why is this bad?

Using a `TypedDict` class in these contexts raises `TypeError` at runtime.

## Examples

```python
from typing_extensions import TypedDict


class Movie(TypedDict):
    name: str
    director: str


def f(arg: object, arg2: type):
    isinstance(arg, Movie)  # error: [isinstance-against-typed-dict]
    issubclass(arg2, Movie)  # error: [isinstance-against-typed-dict]


def g(arg: object):
    match arg:
        case Movie():  # error: [isinstance-against-typed-dict]
            pass
```

## References

- [Typing specification: `TypedDict`](https://typing.python.org/en/latest/spec/typeddict.html)
