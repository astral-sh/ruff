# Comparison: Non boolean returns

Walking through examples:

- `a = A() < B() < C()`

    1. `A() < B() and B() < C()` - split in N comparison
    1. `A()` and `B()` - evaluate outcome types
    1. `bool` and `bool` - evaluate truthiness
    1. `A | B` - union of "first true" types

- `b = 0 < 1 < A() < 3`

    1. `0 < 1 and 1 < A() and A() < 3` - split in N comparison
    1. `True` and `bool` and `A` - evaluate outcome types
    1. `True` and `bool` and `bool` - evaluate truthiness
    1. `bool | A` - union of "true" types

- `c = 10 < 0 < A() < B() < C()` short-circuit to False

```py
from __future__ import annotations

class A:
    def __lt__(self, other) -> A:
        return self

    def __gt__(self, other) -> bool:
        return False

class B:
    def __lt__(self, other) -> B:
        return self

class C:
    def __lt__(self, other) -> C:
        return self

x = A() < B() < C()
reveal_type(x)  # revealed: A & ~AlwaysTruthy | B

y = 0 < 1 < A() < 3
reveal_type(y)  # revealed: Literal[False] | A

z = 10 < 0 < A() < B() < C()
reveal_type(z)  # revealed: Literal[False]
```
