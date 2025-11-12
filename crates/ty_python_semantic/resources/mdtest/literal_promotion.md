# Literal promotion

```toml
[environment]
python-version = "3.12"
```

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

Literal promotion should also not apply recursively to type arguments in contravariant/invariant
position:

```py
class Bivariant[T]:
    pass

class Covariant[T]:
    def pop(self) -> T:
        raise NotImplementedError

class Contravariant[T]:
    def push(self, value: T) -> None:
        pass

class Invariant[T]:
    x: T

def _(
    bivariant: Bivariant[Literal[1]],
    covariant: Covariant[Literal[1]],
    contravariant: Contravariant[Literal[1]],
    invariant: Invariant[Literal[1]],
):
    reveal_type([bivariant])  # revealed: list[Unknown | Bivariant[int]]
    reveal_type([covariant])  # revealed: list[Unknown | Covariant[int]]

    reveal_type([contravariant])  # revealed: list[Unknown | Contravariant[Literal[1]]]
    reveal_type([invariant])  # revealed: list[Unknown | Invariant[Literal[1]]]
```
