# Generic type aliases with legacy type variables

## Mutually recursive implicit aliases through a helper

A generic helper can connect mutually recursive aliases while preserving their tuple elements.

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T")
U = TypeVar("U")
Pair = TypeAliasType("Pair", tuple[T, U], type_params=(T, U))

Outer = tuple[int, "Inner | None"]
Inner = Pair[str, "Outer | None"]

def inspect(value: Outer):
    reveal_type(value)  # revealed: Outer
    if (inner := value[1]) is not None:
        reveal_type(inner[0])  # revealed: str
        if (outer := inner[1]) is not None:
            reveal_type(outer[0])  # revealed: int

def valid() -> Outer:
    return (1, ("leaf", (2, None)))  # no diagnostic

def invalid() -> Outer:
    return (1, ("leaf", ("bad", None)))  # error: [invalid-return-type]
```

## Recursive references in unused helper arguments

A recursive reference in an unused type argument does not change the tuple's element types.

```py
from typing_extensions import TypeAliasType, TypeVar

T = TypeVar("T")
U = TypeVar("U")
First = TypeAliasType("First", T, type_params=(T, U))

Outer = tuple[int, "Inner | None"]
Inner = First[tuple[str, "Outer | None"], "Inner"]

def inspect(outer: Outer, inner: Inner):
    reveal_type(outer)  # revealed: Outer
    reveal_type(inner)  # revealed: Inner
    reveal_type(inner[0])  # revealed: str
    if (next_outer := inner[1]) is not None:
        reveal_type(next_outer[0])  # revealed: int
```
