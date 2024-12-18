# Custom unary operations

## Class method

```py
class Yes:
    def __pos__(self) -> bool:
        return False

    def __neg__(self) -> str:
        return "negative"

    def __invert__(self) -> int:
        return 17

class Sub(Yes): ...
class No: ...

reveal_type(+Yes())  # revealed: bool
reveal_type(-Yes())  # revealed: str
reveal_type(~Yes())  # revealed: int

reveal_type(+Sub())  # revealed: bool
reveal_type(-Sub())  # revealed: str
reveal_type(~Sub())  # revealed: int

# error: [unsupported-operator] "Unary operator `+` is unsupported for type `No`"
reveal_type(+No())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `-` is unsupported for type `No`"
reveal_type(-No())  # revealed: Unknown
# error: [unsupported-operator] "Unary operator `~` is unsupported for type `No`"
reveal_type(~No())  # revealed: Unknown
```
