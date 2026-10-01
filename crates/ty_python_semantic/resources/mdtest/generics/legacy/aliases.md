# Generic type aliases: legacy syntax

## Negated recursive alias specializations

A recursive alias can specialize to `object`, whose negation is `Never`.

```py
from typing import TypeVar
from typing_extensions import Never, assert_type
from ty_extensions import Not

T = TypeVar("T")
A = T | list["A[T]"]

def f(x: Not[A[object]]):
    assert_type(x, Never)
```

Specialization also preserves double-negation elimination when the alias body is recursive.

```py
B = Not[list["B[T]"] | T]

def identity(x: T) -> T:
    return x

def g(x: Not[B[int]]) -> list[B[int]] | int:
    assert_type(x, list[B[int]] | int)
    return identity(x)  # no diagnostic
```
