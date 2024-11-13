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
<https://docs.python.org/3/library/stdtypes.html#truth-value-testing>. An instance is True or False
if the `__bool__` method says so.

The `__len__` method on it's own will not determine the truthiness of an instance because `__bool__`
method can override it.

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

class BoolIsBool:
    __bool__ = bool

# revealed: bool
reveal_type(not BoolIsBool())

class NoBoolMethod: ...

# revealed: bool
reveal_type(not NoBoolMethod())

class LenZero:
    def __len__(self) -> Literal[0]:
        return 0

# revealed: bool
reveal_type(not LenZero())

class LenNonZero:
    def __len__(self) -> Literal[1]:
        return 1

# revealed: bool
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

# revealed: Literal[False]
reveal_type(not WithBothLenAndBool2())

# TODO: raise diagnostic when __bool__ method is not valid: [unsupported-operator] "Method __bool__ for type `MethodBoolInvalid` should return `bool`, returned type `int`"
# https://docs.python.org/3/reference/datamodel.html#object.__bool__
class MethodBoolInvalid:
    def __bool__(self) -> int:
        return 0

# revealed: bool
reveal_type(not MethodBoolInvalid())
```
