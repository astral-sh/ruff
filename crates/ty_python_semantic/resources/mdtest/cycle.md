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
# TODO: Should be `Unknown | int`
reveal_type(p.x)  # revealed: Unknown
# TODO: Should be `Unknown | int`
reveal_type(p.y)  # revealed: Unknown
```

## Implicit instance attributes

This is a regression test for <https://github.com/astral-sh/ty/issues/1111>:

```py
def combine(*args) -> int:
    return 0

class C:
    def __init__(self: "C"):
        self.x1 = 0
        self.x2 = 0
        self.x3 = 0
        self.x4 = 0
        self.x5 = 0

    def f1(self: "C"):
        self.x1 = combine(self.x2, self.x3, self.x4, self.x5)
        self.x2 = combine(self.x1, self.x3, self.x4, self.x5)
        self.x3 = combine(self.x1, self.x2, self.x4, self.x5)
        self.x4 = combine(self.x1, self.x2, self.x3, self.x5)
        self.x5 = combine(self.x1, self.x2, self.x3, self.x4)

    def f2(self: "C"):
        self.x1 = combine(self.x2, self.x3, self.x4, self.x5)
        self.x2 = combine(self.x1, self.x3, self.x4, self.x5)
        self.x3 = combine(self.x1, self.x2, self.x4, self.x5)
        self.x4 = combine(self.x1, self.x2, self.x3, self.x5)
        self.x5 = combine(self.x1, self.x2, self.x3, self.x4)

    def f3(self: "C"):
        self.x1 = combine(self.x2, self.x3, self.x4, self.x5)
        self.x2 = combine(self.x1, self.x3, self.x4, self.x5)
        self.x3 = combine(self.x1, self.x2, self.x4, self.x5)
        self.x4 = combine(self.x1, self.x2, self.x3, self.x5)
        self.x5 = combine(self.x1, self.x2, self.x3, self.x4)

# TODO: should be `Unknown | int`
reveal_type(C().x1)  # revealed: Unknown
```
