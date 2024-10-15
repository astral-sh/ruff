# Non boolean returns

Walking through examples:

- `a = A() < B() < C()`

    1. `A() < B() and B() < C()` - split in N comparison
    1. `A()` and `B()`              - evaluate outcome types
    1. `bool` and `bool`            - evaluate truthiness
    1. `A | B`                    - union of "first true" types

- `b = 0 < 1 < A() < 3`

    1. `0 < 1 and 1 < A() and A() < 3` - split in N comparison
    1. `True` and `bool` and `A` - evaluate outcome types
    1. `True` and `bool` and `bool` - evaluate truthiness
    1. `bool | A` - union of "true" types

- `c = 10 < 0 < A() < B() < C()` short-cicuit to False

```py
from __future__ import annotations
class A:
    def __lt__(self, other) -> A: ...
class B:
    def __lt__(self, other) -> B: ...
class C:
    def __lt__(self, other) -> C: ...

a = A() < B() < C()
b = 0 < 1 < A() < 3
c = 10 < 0 < A() < B() < C()

reveal_type(a)  # revealed: A | B
reveal_type(b)  # revealed: bool | A
reveal_type(c)  # revealed: Literal[False]
```
