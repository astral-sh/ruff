# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
```

## Empty lists with an expected type

An empty list can use the element type from an expected `Sequence`. If it matches more than one
union member, ty keeps the element type only when all matching members agree. Equivalent element
types count as agreement:

```py
from collections.abc import Iterable, Reversible, Sequence
from typing import Protocol

def default_to_empty(items: Sequence[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[int]

annotated: Sequence[int] = []
reveal_type(annotated)  # revealed: list[int]

def same_element_type(items: Iterable[int] | Reversible[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[int]

def different_element_types(items: Sequence[str] | Sequence[bytes] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

class P1(Protocol):
    def method(self) -> int: ...

class P2(Protocol):
    def method(self) -> int: ...

def equivalent_protocols(
    items: Iterable[P1] | Reversible[P2] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[P1]
```

`Any` and `Unknown` do not provide a precise element type, even when nested inside another type. A
separate `Any` union member also leaves the list open to any element type. Reversing the union
members does not change the result:

```py
from collections.abc import Sequence
from typing import Any
from ty_extensions import Unknown

def any_then_int(items: Sequence[Any] | Sequence[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def int_then_any(items: Sequence[int] | Sequence[Any] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def any_then_unknown(items: Sequence[Any] | Sequence[Unknown] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def unknown_then_any(items: Sequence[Unknown] | Sequence[Any] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def nested_any_then_unknown(
    items: Sequence[list[Any]] | Sequence[list[Unknown]] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def nested_unknown_then_any(
    items: Sequence[list[Unknown]] | Sequence[list[Any]] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def top_level_any(items: Sequence[int] | Any | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]
```

The protocol below uses its type parameter for both iteration and containment, but only `__iter__`
constrains the list's element type. Reversing the union members must not change the result:

```py
from collections.abc import Iterable, Iterator
from typing import Protocol, TypeVar

ElementT = TypeVar("ElementT")

class IterableAndContainer(Protocol[ElementT]):
    def __iter__(self) -> Iterator[ElementT]: ...
    def __contains__(self, value: ElementT, /) -> bool: ...

def int_then_str(
    items: IterableAndContainer[int] | IterableAndContainer[str] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def str_then_int(
    items: IterableAndContainer[str] | IterableAndContainer[int] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def iterable_then_list(items: Iterable[int] | list[str] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def list_then_iterable(items: list[str] | Iterable[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]
```

An expected type for a generic call is not applied directly to an empty list passed to that call:

```py
from collections.abc import Iterable, Sequence
from typing import TypeVar

T = TypeVar("T")

def identity(value: T) -> T:
    return value

wrapped_iterable_first: Iterable[int] | Sequence[str] = identity([])
reveal_type(wrapped_iterable_first)  # revealed: list[Unknown]

wrapped_sequence_first: Sequence[str] | Iterable[int] = identity([])
reveal_type(wrapped_sequence_first)  # revealed: list[Unknown]
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
