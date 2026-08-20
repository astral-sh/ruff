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

Concatenating two statically empty built-in tuples preserves their fixed-length shape.

```py
def concatenate_empty(left: tuple[()], right: tuple[()]) -> None:
    reveal_type(left + right)  # revealed: tuple[()]
```

Direct builtin method calls still use the generic homogeneous return type.

```py
# TODO: This should also preserve the exact empty tuple result.
reveal_type(().__add__(()))  # revealed: tuple[Never, ...]
```

An empty tuple subclass can override ordinary or reflected addition, so its methods must still
determine the result.

```py
class EmptyTupleWithAddition(tuple[()]):
    def __add__(self, other: object) -> "EmptyTupleWithAddition":
        return self

class EmptyTupleWithReflectedAddition(tuple[()]):
    def __radd__(self, other: object) -> str:
        return "reflected"

def concatenate_subclasses(
    left: EmptyTupleWithAddition,
    right: EmptyTupleWithReflectedAddition,
) -> None:
    reveal_type(left + ())  # revealed: EmptyTupleWithAddition
    reveal_type(left.__add__(()))  # revealed: EmptyTupleWithAddition
    reveal_type(right.__radd__(()))  # revealed: str
    # TODO: Preserve the tuple literal's exact runtime class. Its inferred `tuple[()]`
    # type admits subclasses, so we cannot yet rule out calling `tuple.__add__`.
    reveal_type(() + right)  # revealed: str | tuple[Never, ...]
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

## Repetition of empty tuples

Repeating a statically empty built-in tuple by an integer literal preserves its fixed-length shape,
regardless of the multiplier or operand order.

```py
reveal_type(() * 0)  # revealed: tuple[()]
reveal_type(() * 3)  # revealed: tuple[()]
reveal_type(() * -1)  # revealed: tuple[()]
reveal_type(3 * ())  # revealed: tuple[()]
reveal_type(() * True)  # revealed: tuple[()]
reveal_type(False * ())  # revealed: tuple[()]

def repeat_empty(value: tuple[()]) -> None:
    reveal_type(value * 2)  # revealed: tuple[()]
    reveal_type(2 * value)  # revealed: tuple[()]
```

TODO: Preserve the exact empty result for non-literal multipliers and direct builtin method calls.

```py
def repeat_non_literal(multiplier: int) -> None:
    reveal_type(() * multiplier)  # revealed: tuple[Never, ...]
    reveal_type(multiplier * ())  # revealed: tuple[Never, ...]

reveal_type(().__mul__(2))  # revealed: tuple[Never, ...]
reveal_type(().__rmul__(2))  # revealed: tuple[Never, ...]
```

Tuple subclasses can also override repetition in either operand order.

```py
class EmptyTupleWithRepetition(tuple[()]):
    def __mul__(self, other: object) -> "EmptyTupleWithRepetition":
        return self

    def __rmul__(self, other: object) -> "EmptyTupleWithRepetition":
        return self

def repeat_subclass(value: EmptyTupleWithRepetition) -> None:
    reveal_type(value * 2)  # revealed: EmptyTupleWithRepetition
    reveal_type(2 * value)  # revealed: EmptyTupleWithRepetition
```
