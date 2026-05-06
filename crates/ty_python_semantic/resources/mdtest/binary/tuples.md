# Binary operations on tuples

## Concatenation for heterogeneous tuples

```py
reveal_type((1, 2) + (3, 4))  # revealed: tuple[Literal[1], Literal[2], Literal[3], Literal[4]]
reveal_type(() + (1, 2))  # revealed: tuple[Literal[1], Literal[2]]
reveal_type((1, 2) + ())  # revealed: tuple[Literal[1], Literal[2]]
reveal_type(() + ())  # revealed: tuple[()]

def _(x: tuple[int, str], y: tuple[None, tuple[int]]):
    reveal_type(x + y)  # revealed: tuple[int, str, None, tuple[int]]
    reveal_type(y + x)  # revealed: tuple[None, tuple[int], int, str]

def f(x: tuple[int, int]) -> tuple[str, int, int]:
    return ("asdf",) + x
```

## Concatenation for homogeneous tuples

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

## Conditional augmented concatenation

Large control-flow joins should not infer every possible tuple shape.

```py
def flag() -> bool:
    return True

t = ()
if flag():
    t += (1,)
if flag():
    t += (2,)
if flag():
    t += (3,)
if flag():
    t += (4,)
if flag():
    t += (5,)
if flag():
    t += (6,)
if flag():
    t += (7,)
if flag():
    t += (8,)

reveal_type(t)  # revealed: tuple[Literal[1, 2, 3, 4, 5, 6, 7, 8], ...]
```

## Augmented concatenation in loops

Cycle recovery should not infer an increasingly precise tuple shape on every iteration.

```py
from collections.abc import Hashable, Mapping
from typing import TypeAlias

Key: TypeAlias = tuple[str, ...]

def flag() -> bool:
    return True

def parse_key(key_part: str) -> Key:
    key: Key = (key_part,)
    while flag():
        key += (key_part,)
    reveal_type(key)  # revealed: tuple[str, ...]
    return key

def append_flag(state: tuple[object, ...], alias_is_default: bool) -> tuple[object, ...]:
    while flag():
        state = (*state, alias_is_default)
    reveal_type(state)  # revealed: tuple[object, ...]
    return state

_marker: tuple[object] = (object(),)

def make_key(args: tuple[object, ...], kwds: Mapping[object, object]) -> Hashable:
    key: tuple[object, ...] = args
    if kwds:
        key += _marker
        for item in kwds.items():
            key += item
    reveal_type(key)  # revealed: tuple[object, ...]
    return key

def memoized_key(
    names: str,
    dtype: object | None,
    ilp64: str,
    arrays: tuple[object, ...],
) -> tuple[object, ...]:
    key = (names, dtype, ilp64)
    for array in arrays:
        key += (array, flag())
    reveal_type(key)  # revealed: tuple[object, ...]
    return key

def piecewise_cases(cdf: Mapping[object, object]) -> tuple[tuple[object, object], ...]:
    cases = ((None, False),)
    for key, value in cdf.items():
        cases = cases + ((key, value),)
    reveal_type(cases)  # revealed: tuple[tuple[object, object], ...]
    return cases
```
