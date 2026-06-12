## What it does

Checks for various `@overload`-decorated functions that have non-stub bodies.

## Why is this bad?

Functions decorated with `@overload` are ignored at runtime; they are overridden
by the implementation function that follows the series of overloads. While it is
not illegal to provide a body for an `@overload`-decorated function, it may indicate
a misunderstanding of how the `@overload` decorator works.

## Example

```py
from typing import overload


@overload
def foo(x: int) -> int:
    return x + 1  # will never be executed


@overload
def foo(x: str) -> str:
    return "Oh no, got a string"  # will never be executed


def foo(x: int | str) -> int | str:
    raise Exception("unexpected type encountered")
```

Use instead:

```py
from typing import assert_never, overload


@overload
def foo(x: int) -> int: ...


@overload
def foo(x: str) -> str: ...


def foo(x: int | str) -> int | str:
    if isinstance(x, int):
        return x + 1
    elif isinstance(x, str):
        return "Oh no, got a string"
    else:
        assert_never(x)
```

## References

- [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)
