# Binary operations on tuples

## Concatenation for heterogeneous tuples

Concatenating fixed-length tuples preserves the type and order of their elements. An empty tuple
contributes no elements.

This inference is not strictly sound: a value annotated as `tuple` can be an instance of a subclass.
We assume such subclasses do not override `__add__` or `__radd__` with behavior incompatible with
ordinary tuple concatenation.

```py
reveal_type((1, 2) + (3, 4))  # revealed: tuple[Literal[1], Literal[2], Literal[3], Literal[4]]
reveal_type(() + (1, 2))  # revealed: tuple[Literal[1], Literal[2]]
reveal_type((1, 2) + ())  # revealed: tuple[Literal[1], Literal[2]]
reveal_type(() + ())  # revealed: tuple[()]

def _(x: tuple[int, str], y: tuple[None, tuple[int]]):
    reveal_type(x + y)  # revealed: tuple[int, str, None, tuple[int]]
    reveal_type(y + x)  # revealed: tuple[None, tuple[int], int, str]
```

## Concatenation for homogeneous tuples

Concatenating variable-length tuples combines their element types. Fixed-length operands preserve a
known prefix or suffix; elements between two variable-length portions become part of the variable
portion.

```py
def _(x: tuple[int, ...], y: tuple[str, ...]):
    reveal_type(x + x)  # revealed: tuple[int, ...]
    reveal_type(x + y)  # revealed: tuple[int | str, ...]
    reveal_type((1, 2) + x)  # revealed: tuple[Literal[1], Literal[2], *tuple[int, ...]]
    reveal_type(x + (3, 4))  # revealed: tuple[*tuple[int, ...], Literal[3], Literal[4]]
    reveal_type((1, 2) + x + (3, 4))  # revealed: tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[3], Literal[4]]
    reveal_type((1, 2) + y + (3, 4) + x)  # revealed: tuple[Literal[1], Literal[2], *tuple[int | str, ...]]
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
    reveal_type(one_two + x)  # revealed: tuple[Literal[1], Literal[2], *tuple[int, ...]]
    reveal_type(x + three_four)  # revealed: tuple[*tuple[int, ...], Literal[3], Literal[4]]
    reveal_type(one_two + x + three_four)  # revealed: tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[3], Literal[4]]
    reveal_type(one_two + y + three_four + x)  # revealed: tuple[Literal[1], Literal[2], *tuple[int | str, ...]]
```

## Union operands

Concatenation preserves the possible lengths and element types of each tuple in a union.

```py
def concatenate(left: tuple[int] | tuple[str, str], right: tuple[bytes]) -> None:
    reveal_type(left + right)  # revealed: tuple[int, bytes] | tuple[str, str, bytes]
    reveal_type(right + left)  # revealed: tuple[bytes, int] | tuple[bytes, str, str]
```

## Tuple subclass overrides

An explicitly known tuple subclass can override addition. Its `__add__` return annotation determines
the result. On the right, its `__radd__` can take precedence over the left operand's `__add__`; both
results remain possible because a `tuple` annotation also admits subclass instances.

```py
class Custom(tuple[int]):
    def __add__(self, other: object) -> tuple[int, int]:
        return (1, 2)

    def __radd__(self, other: object) -> str:
        return "custom"

def add_custom(custom: Custom, plain: tuple[str]) -> None:
    reveal_type(custom + plain)  # revealed: tuple[int, int]
    reveal_type(plain + custom)  # revealed: str | tuple[str | int, ...]
```

## Many conditional concatenations

Independent conditions can produce exponentially many tuple shapes. For large combinations,
concatenation approximates the result as a variable-length tuple to keep inference bounded. Other
union alternatives still use their operator methods.

```py
def flag() -> bool:
    return True

class Custom(tuple[str]):
    def __add__(self, other: object) -> "Custom":
        return self

def initial() -> Custom | tuple[()]:
    return ()

value = ()
mixed = initial()
if flag():
    value += (1,)
    mixed = mixed + (1,)
if flag():
    value += (2,)
    mixed = mixed + (2,)
if flag():
    value += (3,)
    mixed = mixed + (3,)
if flag():
    value += (4,)
    mixed = mixed + (4,)
if flag():
    value += (5,)
    mixed = mixed + (5,)
if flag():
    value += (6,)
    mixed = mixed + (6,)

reveal_type(value)  # revealed: tuple[Literal[6, 1, 2, 3, 4, 5], ...]
reveal_type(mixed)  # revealed: Custom | tuple[Literal[6, 1, 2, 3, 4, 5], ...]
```

## Concatenating two large unions

Each operand has six tuple shapes, so concatenation can produce 36 combinations. Both ordinary and
augmented assignment use a variable-length approximation when combining these alternatives.

```py
Choice = (
    tuple[int, int, str]
    | tuple[int, str, int]
    | tuple[str, int, int]
    | tuple[int, str, str]
    | tuple[str, int, str]
    | tuple[str, str, int]
)

def concatenate(left: Choice, right: Choice) -> None:
    reveal_type(left + right)  # revealed: tuple[int | str, ...]
    result = left
    result += right
    reveal_type(result)  # revealed: tuple[int | str, ...]
```

Non-tuple alternatives still use their addition and reflected addition methods, even when the tuple
combinations require approximation.

```py
class Custom:
    def __add__(self, other: object) -> "Custom":
        return self

    def __radd__(self, other: object) -> bytes:
        return b"custom"

def concatenate_custom(left: Choice | Custom, right: Choice | Custom) -> None:
    reveal_type(left + right)  # revealed: bytes | tuple[int | str, ...] | Custom
    result = left
    result += right
    reveal_type(result)  # revealed: bytes | tuple[int | str, ...] | Custom
```

## Repeated doubling

Doubling a tuple repeatedly grows its length exponentially without creating a union. Very large
concatenations also use a variable-length approximation.

```py
value = (1,)
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
value = value + value
reveal_type(value)  # revealed: tuple[Literal[1], ...]
```
