## What it does

Checks for overload implementations that are incompatible with one or more overloads.

After decorators are applied, the implementation must accept every call allowed by the overloads.
Each overload's return type must be assignable to the implementation's return type.

## Why is this bad?

Callers rely on the overloads to know which arguments they can pass and what type to expect in
return.

## Examples

### Incompatible parameters

```py
from typing import overload


@overload
def read() -> str: ...  # error
@overload
def read(size: int) -> str: ...
def read(size: int) -> str:
    return "x" * size
```

The first overload allows `read()`. At runtime, that call raises `TypeError` because the
implementation requires a `size` argument.

### Incompatible return type

```py
from typing import overload


@overload
def double(value: int) -> int: ...  # error
@overload
def double(value: str) -> str: ...
def double(value: int | str) -> str:
    return str(value) * 2
```

The first overload promises an `int`, but `double(3)` returns the string `"33"`.

### A decorator makes the implementation uncallable

```py
from collections.abc import Callable
from typing import overload

handlers: list[Callable[..., str]] = []


def register(func: Callable[..., str]) -> None:
    handlers.append(func)


@overload
def greet() -> str: ...
@overload
def greet(name: str) -> str: ...
@register
def greet(name: str = "world") -> str:  # error
    return f"Hello, {name}!"
```

`register` returns `None`, so `@register` leaves `greet` bound to `None`. Calling `greet()` raises
`TypeError`.

## References

- [Typing specification: Implementation consistency](https://typing.python.org/en/latest/spec/overload.html#implementation-consistency)
