# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
```

A directly contextualized empty list uses a fully static covariant element type when every
compatible union arm agrees:

```py
from collections.abc import Iterable, Reversible, Sequence

def default_to_empty(items: Sequence[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[int]
    reveal_type(items)  # revealed: Sequence[int]

def agreeing(items: Iterable[int] | Reversible[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[int]

def conflicting(items: Sequence[str] | Sequence[bytes] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]
```

Semantically equivalent fully static element types also count as agreement:

```py
from collections.abc import Iterable
from typing import Protocol, TypeVar

ElementT_co = TypeVar("ElementT_co", covariant=True)

class SupportsGetItem(Protocol[ElementT_co]):
    def __getitem__(self, index: int, /) -> ElementT_co: ...

class P1(Protocol):
    def method(self) -> int: ...

class P2(Protocol):
    def method(self) -> int: ...

def equivalent_protocols(
    items: Iterable[P1] | SupportsGetItem[P2] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[P1]
```

Dynamic element types, including nested dynamic types, retain the normal `Unknown` fallback
independent of union order:

```py
from collections.abc import Sequence
from typing import Any
from ty_extensions import Unknown

def any_first(items: Sequence[Any] | Sequence[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def int_first(items: Sequence[int] | Sequence[Any] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def any_unknown_first(items: Sequence[Any] | Sequence[Unknown] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def unknown_any_first(items: Sequence[Unknown] | Sequence[Any] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def nested_any_first(
    items: Sequence[list[Any]] | Sequence[list[Unknown]] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def nested_unknown_first(
    items: Sequence[list[Unknown]] | Sequence[list[Any]] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]
```

The effective variance of a structural constraint determines whether the fallback applies. Mixed
covariant and invariant arms remain order-independent:

```py
from collections.abc import Iterable, Iterator
from typing import Protocol, TypeVar

ElementT = TypeVar("ElementT")

class IterableConsumer(Protocol[ElementT]):
    def __iter__(self) -> Iterator[ElementT]: ...
    def __contains__(self, value: ElementT, /) -> bool: ...

def protocol_int_first(
    items: IterableConsumer[int] | IterableConsumer[str] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def protocol_str_first(
    items: IterableConsumer[str] | IterableConsumer[int] | None = None,
) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def covariant_first(items: Iterable[int] | list[str] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]

def invariant_first(items: list[str] | Iterable[int] | None = None) -> None:
    if items is None:
        items = []
        reveal_type(items)  # revealed: list[Unknown]
```

Covariant context is not propagated through an enclosing generic call:

```py
from collections.abc import Iterable, Sequence
from typing import TypeVar

T = TypeVar("T")

def identity(value: T) -> T:
    return value

wrapped_int_first: Iterable[int] | Sequence[str] = identity([])
reveal_type(wrapped_int_first)  # revealed: list[Unknown]

wrapped_str_first: Sequence[str] | Iterable[int] = identity([])
reveal_type(wrapped_str_first)  # revealed: list[Unknown]
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
