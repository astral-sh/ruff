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
