# Generic type aliases with legacy type variables

## Invalid arguments in mutually recursive aliases

An invalid argument to a generic helper is diagnosed without preventing inference of the other
elements of mutually recursive aliases.

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T")
U = TypeVar("U")
Pair = TypeAliasType("Pair", tuple[T, U], type_params=(T, U))

not_a_type = 42
Outer = tuple[int, "Inner | None"]
Inner = Pair[not_a_type, "Outer | None"]  # error: [invalid-type-form]

def inspect(value: Outer):
    reveal_type(value[0])  # revealed: int
    if (inner := value[1]) is not None:
        reveal_type(inner[0])  # revealed: Unknown
        if (outer := inner[1]) is not None:
            reveal_type(outer[0])  # revealed: int

def valid() -> Outer:
    return (1, ("leaf", (2, None)))  # no diagnostic

def invalid() -> Outer:
    return (1, ("leaf", ("bad", None)))  # error: [invalid-return-type]
```
