# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
reveal_type(type([]))  # revealed: <class 'list'>
reveal_type(bool([]))  # revealed: Literal[False]
reveal_type(type(list[int]()))  # revealed: <class 'list'>
reveal_type(bool(list[int]()))  # revealed: Literal[False]
```

The runtime class remains exact when two exact lists are concatenated. If either operand could be a
list subclass, the result could be produced by an overridden `__add__` or `__radd__` method and is
therefore not exact:

```py
left = [1]
right = [2]
reveal_type(type(left + right))  # revealed: <class 'list'>
reveal_type(bool([] + []))  # revealed: Literal[False]
reveal_type(bool([1] + []))  # revealed: Literal[True]

def concatenate(left: list[int], right: list[int]) -> None:
    reveal_type(type(left + right))  # revealed: type[list[int]]
    reveal_type(type([1] + right))  # revealed: type[list[int]]
    reveal_type(type(left + [1]))  # revealed: type[list[int]]
    reveal_type(bool([] + right))  # revealed: bool
```

Exact runtime class survives storage, but cardinality does not. A list can be mutated through the
stored place or another alias, so definite cardinality is retained only on fresh expressions and
results:

```py
empty = []
reveal_type(type(empty))  # revealed: <class 'list'>
reveal_type(bool(empty))  # revealed: bool

nested = [[]]
reveal_type(bool(nested[0]))  # revealed: bool
```

## List of tuples

```py
reveal_type([(1, 2), (3, 4)])  # revealed: list[tuple[int, int]]
```

## List of functions

```py
def a(_: int) -> int:
    return 0

def b(_: int) -> int:
    return 1

x = [a, b]
reveal_type(x)  # revealed: list[(_: int) -> int]
```

The inferred `Callable` type is function-like, i.e. we can still access attributes like `__name__`:

```py
reveal_type(x[0].__name__)  # revealed: str
```

## Mixed list

```py
# revealed: list[int | tuple[int, ...]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## None promotion

`None` is promoted to `None | Unknown` in list literals when it is the only element type, so that
the inferred type does not overly restrict subsequent mutations of the list.

```py
from typing import Sequence

reveal_type([None])  # revealed: list[None | Unknown]
reveal_type([1, None])  # revealed: list[int | None]
reveal_type([(None,)])  # revealed: list[tuple[None | Unknown]]
reveal_type([[None], [None]])  # revealed: list[list[None | Unknown]]

x: list[int | None] = [None]
reveal_type(x)  # revealed: list[int | None]

y: list[tuple[int | None, ...]] = [(None,)]
reveal_type(y)  # revealed: list[tuple[int | None, ...]]

z: list[Sequence[int | str | None]] = [(None,), [None], (None, None)]
reveal_type(z)  # revealed: list[Sequence[int | str | None]]

xx: list[None] = reveal_type([None])  # revealed: list[None]
reveal_type(xx)  # revealed: list[None]

yy = reveal_type([None])  # revealed: list[None | Unknown]
reveal_type(yy)  # revealed: list[None | Unknown]

# Bare `list` in a type expression is equivalent to `list[Unknown]`
zz: list = [None]  # error: [missing-type-argument]
reveal_type(zz)  # revealed: list[Unknown]

# Promotion only happens if we're in invariant contexts,
# same as with `Literal` types:
reveal_type((1, 2, None))  # revealed: tuple[Literal[1], Literal[2], None]
reveal_type(((((None,),),),))  # revealed: tuple[tuple[tuple[tuple[None]]]]
reveal_type((((([None],),),),))  # revealed: tuple[tuple[tuple[tuple[list[None | Unknown]]]]]

class Foo:
    def __init__(self):
        self.mylist = [None, None, None]

    def method(self):
        self.mylist[0] = 42

reveal_type(Foo().mylist)  # revealed: list[None | Unknown]
```

## List comprehensions

```py
reveal_type([x for x in range(42)])  # revealed: list[int]
```
