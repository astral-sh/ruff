# Cycles

## Function signature

Deferred annotations can result in cycles in resolving a function signature:

```py
from __future__ import annotations

# error: [invalid-type-form]
def f(x: f):
    pass

reveal_type(f)  # revealed: def f(x: Unknown) -> Unknown
```

## Unpacking

See: <https://github.com/astral-sh/ty/issues/364>

```py
class Point:
    def __init__(self, x: int = 0, y: int = 0) -> None:
        self.x = x
        self.y = y

    def replace_with(self, other: "Point") -> None:
        self.x, self.y = other.x, other.y

p = Point()
reveal_type(p.x)  # revealed: Unknown | int
reveal_type(p.y)  # revealed: Unknown | int
```

## Self-referential bare type alias

```py
A = list["A" | None]

def f(x: A):
    # TODO: should be `list[A | None]`?
    reveal_type(x)  # revealed: list[Divergent]
    # TODO: should be `A | None`?
    reveal_type(x[0])  # revealed: Divergent
```

## Self-referential type variables

```py
from typing import Generic, TypeVar

B = TypeVar("B", bound="Base")

# TODO: no error
# error: [invalid-argument-type] "`typing.TypeVar | typing.TypeVar` is not a valid argument to `Generic`"
class Base(Generic[B]):
    pass
```
