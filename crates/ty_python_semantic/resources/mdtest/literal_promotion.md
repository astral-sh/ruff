# Literal promotion

There are certain places where we promote literals to their common supertype:

```py
reveal_type([1, 2, 3])  # revealed: list[Unknown | int]
reveal_type({"a", "b", "c"})  # revealed: set[Unknown | str]
```

This promotion should not take place if the literal type appears in contravariant position:

```py
from typing import Callable, Literal

def in_negated_position(non_zero_number: int):
    if non_zero_number == 0:
        raise ValueError()

    reveal_type(non_zero_number)  # revealed: int & ~Literal[0]

    reveal_type([non_zero_number])  # revealed: list[Unknown | (int & ~Literal[0])]

def in_parameter_position(callback: Callable[[Literal[1]], None]):
    reveal_type(callback)  # revealed: (Literal[1], /) -> None

    reveal_type([callback])  # revealed: list[Unknown | ((Literal[1], /) -> None)]

def double_negation(callback: Callable[[Callable[[Literal[1]], None]], None]):
    reveal_type(callback)  # revealed: ((Literal[1], /) -> None, /) -> None

    reveal_type([callback])  # revealed: list[Unknown | (((int, /) -> None, /) -> None)]
```
