# Unary Operations

## Invert, UAdd, USub

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

a = Number()

reveal_type(+a)  # revealed: int
reveal_type(-a)  # revealed: int
reveal_type(~a)  # revealed: Literal[True]

class NoDunder: ...

b = NoDunder()
+b  # error: [unsupported-operator] "Unary operator `+` is unsupported for type `NoDunder`"
-b  # error: [unsupported-operator] "Unary operator `-` is unsupported for type `NoDunder`"
~b  # error: [unsupported-operator] "Unary operator `~` is unsupported for type `NoDunder`"
```

## Not

Not operator is inferred based on
<https://docs.python.org/3/library/stdtypes.html#truth-value-testing>. All objects are true unless
the `__bool__` or `__len__` is returning `False`.

[`__len__`](https://docs.python.org/3/reference/datamodel.html#object.__len__) method should return
an integer and [`__bool__`](https://docs.python.org/3/reference/datamodel.html#object.__bool__)
method must return a boolean.

```py
class AlwaysTrue:
    def __bool__(self) -> Literal[True]:
        return True

# revealed: Literal[False]
reveal_type(not AlwaysTrue())

class AlwaysFalse:
    def __bool__(self) -> Literal[False]:
        return False

# revealed: Literal[True]
reveal_type(not AlwaysFalse())

class NoBoolMethod: ...

# revealed: Literal[False]
reveal_type(not NoBoolMethod())

class LenZero:
    def __len__(self) -> Literal[0]:
        return 0

# revealed: Literal[True]
reveal_type(not LenZero())

class LenNonZero:
    def __len__(self) -> Literal[1]:
        return 1

# revealed: Literal[False]
reveal_type(not LenNonZero())

class WithBothLenAndBool1:
    def __bool__(self) -> Literal[False]:
        return False

    def __len__(self) -> Literal[2]:
        return 2

# revealed: Literal[True]
reveal_type(not WithBothLenAndBool1())

class WithBothLenAndBool2:
    def __bool__(self) -> Literal[True]:
        return True

    def __len__(self) -> Literal[0]:
        return 0

# revealed: Literal[True]
reveal_type(not WithBothLenAndBool2())

class MethodBoolInvalid:
    def __bool__(self) -> int:
        return 0

# error: [unsupported-operator] "Method __bool__ for type `MethodBoolInvalid` should return `bool`, returned type `int`"
# revealed: bool
reveal_type(not MethodBoolInvalid())

class MethodLenInvalid:
    def __len__(self) -> float:
        return 0.0

# error: [unsupported-operator] "Method __len__ for type `MethodLenInvalid` should return `int`, returned type `float`"
# revealed: bool
reveal_type(not MethodLenInvalid())
```
