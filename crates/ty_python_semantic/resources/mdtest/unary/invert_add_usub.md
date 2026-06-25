# Invert, UAdd, USub

## Instance

```py
from typing import Literal

class Number:
    def __init__(self, value: int):
        self.value = 1

    def __pos__(self) -> int:
        return +self.value

    def __neg__(self) -> int:
        return -self.value

    def __invert__(self) -> Literal[True]:
        return True

a = Number(0)

reveal_type(+a)  # revealed: int
reveal_type(-a)  # revealed: int
reveal_type(~a)  # revealed: Literal[True]

class NoDunder: ...

b = NoDunder()
+b  # error: [unsupported-operator] "Unary operator `+` is not supported for object of type `NoDunder`"
-b  # error: [unsupported-operator] "Unary operator `-` is not supported for object of type `NoDunder`"
~b  # error: [unsupported-operator] "Unary operator `~` is not supported for object of type `NoDunder`"
```

## TypeVar with bounds

TypeVars with bounds support unary operators if the bound type supports them.

### TypeVar with `float` bound

Since `float` is treated as `int | float` in type annotations, TypeVars bounded by `float` should
support unary `+` and `-` operators:

```py
from typing import TypeVar

T = TypeVar("T", bound=float)

def neg_float_bound(a: T) -> float:
    reveal_type(-a)  # revealed: int | float
    return -a

def pos_float_bound(a: T) -> float:
    reveal_type(+a)  # revealed: int | float
    return +a
```

### TypeVar with `int` bound

TypeVars bounded by `int` should support all unary numeric operators:

```py
from typing import TypeVar

U = TypeVar("U", bound=int)

def neg_int_bound(a: U) -> int:
    reveal_type(-a)  # revealed: int
    return -a

def invert_int_bound(a: U) -> int:
    reveal_type(~a)  # revealed: int
    return ~a
```

### Constrained TypeVar

Constrained TypeVars support unary operators if all constraints support them. When the operator
returns the same type for each constraint (e.g., `-int -> int`), the TypeVar is preserved:

```py
from typing import TypeVar

V = TypeVar("V", int, float)

def neg_constrained(a: V) -> V:
    reveal_type(-a)  # revealed: V@neg_constrained
    return -a
```
