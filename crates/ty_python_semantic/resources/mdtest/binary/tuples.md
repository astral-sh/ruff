# Binary operations on tuples

## Concatenation for heterogeneous tuples

```py
reveal_type((1, 2) + (3, 4))  # revealed: tuple[Literal[1, 2, 3, 4], ...]
reveal_type(() + (1, 2))  # revealed: tuple[Literal[1, 2], ...]
reveal_type((1, 2) + ())  # revealed: tuple[Literal[1, 2], ...]
reveal_type(() + ())  # revealed: tuple[()]

def _(x: tuple[int, str], y: tuple[None, tuple[int]]):
    reveal_type(x + y)  # revealed: tuple[int | str | None | tuple[int], ...]
    reveal_type(y + x)  # revealed: tuple[None | tuple[int] | int | str, ...]
```

## Concatenation for homogeneous tuples

```py
def _(x: tuple[int, ...], y: tuple[str, ...]):
    reveal_type(x + x)  # revealed: tuple[int, ...]
    reveal_type(x + y)  # revealed: tuple[int | str, ...]
    reveal_type((1, 2) + x)  # revealed: tuple[int, ...]
    reveal_type(x + (3, 4))  # revealed: tuple[int, ...]
    reveal_type((1, 2) + x + (3, 4))  # revealed: tuple[int, ...]
    reveal_type((1, 2) + y + (3, 4) + x)  # revealed: tuple[int | str, ...]
```

We get the same results even when we use a legacy type alias, even though this involves first
inferring the `tuple[...]` expression as a value form. (Doing so gives a generic alias of the
`tuple` type, but as a special case, we include the full detailed tuple element specification in
specializations of `tuple`.)

```py
from typing import Literal

OneTwo = tuple[Literal[1], Literal[2]]
ThreeFour = tuple[Literal[3], Literal[4]]
IntTuple = tuple[int, ...]
StrTuple = tuple[str, ...]

def _(one_two: OneTwo, x: IntTuple, y: StrTuple, three_four: ThreeFour):
    reveal_type(x + x)  # revealed: tuple[int, ...]
    reveal_type(x + y)  # revealed: tuple[int | str, ...]
    reveal_type(one_two + x)  # revealed: tuple[int, ...]
    reveal_type(x + three_four)  # revealed: tuple[int, ...]
    reveal_type(one_two + x + three_four)  # revealed: tuple[int, ...]
    reveal_type(one_two + y + three_four + x)  # revealed: tuple[int | str, ...]
```
