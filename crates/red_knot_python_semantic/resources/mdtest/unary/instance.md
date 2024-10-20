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

Not operator is inferred based on <https://docs.python.org/3/library/stdtypes.html#truth-value-testing>
Unless the `__bool__` or `__len__` is returning `False` it's always true.

```py
class AlwaysTrue:
    def __bool__(self) -> Literal[True]:
        return True


# error: [invalid-method] "Method __bool__ for type `AlwaysTrue` returns type `@Todo` rather than `bool`"
# revealed: bool
reveal_type(not AlwaysTrue())


class AlwaysFalse:
    def __bool__(self) -> Literal[False]:
        return False


# error: [invalid-method] "Method __bool__ for type `AlwaysFalse` returns type `@Todo` rather than `bool`"
# revealed: bool
reveal_type(not AlwaysFalse())


class NoBoolMethod: ...


# revealed: Literal[False]
reveal_type(not NoBoolMethod())
```
