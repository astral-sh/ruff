# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
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

## Lists of functions with different parameter lists

The inferred union keeps both signatures if each accepts a call that the other rejects.

```pyi
def left(x: int) -> int: ...
def right(y: int) -> int: ...
def two(x: int, y: int) -> int: ...
def optional(x: int, y: int = 0) -> int: ...
def positional(y: int, /) -> int: ...
def other(y: int = 0) -> int: ...
def fallback(y: int = 0, *args: object, **kwargs: object) -> int: ...

reveal_type([left, right])  # revealed: list[((x: int) -> int) | ((y: int) -> int)]
reveal_type([left, two])  # revealed: list[((x: int) -> int) | ((x: int, y: int) -> int)]
reveal_type([left, other])  # revealed: list[((x: int) -> int) | ((y: int = 0) -> int)]

# All three pairs still simplify to one signature.
reveal_type([left, fallback])  # revealed: list[(x: int) -> int]
reveal_type([left, optional])  # revealed: list[(x: int) -> int]
reveal_type([left, positional])  # revealed: list[(y: int, /) -> int]
```

A later overload can accept every call to `left` even when the first overload cannot.

```pyi
from typing import overload

@overload
def one_or_two(x: int, y: int) -> int: ...
@overload
def one_or_two(x: int) -> int: ...

reveal_type([left, one_or_two])  # revealed: list[(x: int) -> int]
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
