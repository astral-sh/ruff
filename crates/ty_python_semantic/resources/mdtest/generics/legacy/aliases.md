# Generic type aliases: legacy syntax

## Negated recursive alias specializations

A recursive alias can specialize to `object`, whose negation is `Never`.

```py
from typing import TypeVar
from typing_extensions import Never, assert_type
from ty_extensions import Not

T = TypeVar("T")
A = T | list["A[T]"]

def f(x: Not[A[object]]):
    assert_type(x, Never)
```

Specialization also preserves double-negation elimination when the alias body is recursive.

```py
B = Not[list["B[T]"] | T]

def identity(x: T) -> T:
    return x

def g(x: Not[B[int]]) -> list[B[int]] | int:
    assert_type(x, list[B[int]] | int)
    return identity(x)  # no diagnostic
```

## Recursive aliases in restricted complements

A negated recursive alias can share an intersection with a positive constraint. The result keeps the
exclusion: since `Any` can materialize differently in each union member, the two members do not
simplify to `list[Any]`.

```py
from typing import Any, TypeVar
from typing_extensions import TypeAliasType
from ty_extensions import Intersection, Not

T = TypeVar("T")
Without = TypeAliasType("Without", Intersection[list[Any], Not[T]], type_params=(T,))
A = TypeAliasType("A", "list[Without[A] | A]")

def f(x: A):
    reveal_type(x[0])  # revealed: (list[Any] & ~A) | list[Without[A] | A]
```

## Recursive complements after specialization

Specializing the positive constraint to `object` removes it. The remaining complement and the
recursive alias together cover every object.

```py
from typing import TypeVar
from typing_extensions import TypeAliasType, assert_type
from ty_extensions import Intersection, Not

T = TypeVar("T")
U = TypeVar("U")
Without = TypeAliasType("Without", Intersection[T, Not[U]], type_params=(T, U))
A = TypeAliasType("A", "list[Without[object, A] | A]")

def f(x: A):
    assert_type(x[0], object)
```

## Recursive tuples intersected with a protocol

Specialization can put a recursive tuple alias inside a positive intersection. The tuple and its
elements retain that alias, so they can be inspected without repeatedly unfolding the tuple.

```py
from collections.abc import Iterator
from typing import Any, Protocol, TypeVar
from typing_extensions import TypeAliasType, assert_type
from ty_extensions import Intersection

class Items(Protocol):
    def __iter__(self) -> Iterator[Any]: ...

T = TypeVar("T")
WithItems = TypeAliasType("WithItems", Intersection[Items, T], type_params=(T,))
A = TypeAliasType("A", "tuple[WithItems[A], ...]")

def f(x: A):
    reveal_type(x)  # revealed: tuple[WithItems[A], ...]
    assert_type(x[0], WithItems[A])
    y: int = x[0]  # error: [invalid-assignment]
```

The standard `Iterable` protocol exercises the same behavior with its typeshed definition.

```py
from collections.abc import Iterable

WithIterable = TypeAliasType("WithIterable", Intersection[Iterable[Any], T], type_params=(T,))
B = TypeAliasType("B", "tuple[WithIterable[B], ...]")

def g(x: B):
    reveal_type(x)  # revealed: tuple[WithIterable[B], ...]
    assert_type(x[0], WithIterable[B])
```

## Simplifying aliased intersection elements after specialization

Specializing an intersection exposes the types behind its aliases. Subtracting one boolean literal
from `bool` leaves the other. A literal string that is not truthy is empty.

```py
from typing import Literal, TypeVar
from typing_extensions import LiteralString, TypeAliasType, assert_type
from ty_extensions import AlwaysTruthy, Intersection, Not

T = TypeVar("T")
U = TypeVar("U")
Minus = TypeAliasType("Minus", Intersection[T, Not[U]], type_params=(T, U))
Boolean = TypeAliasType("Boolean", bool)
TrueLiteral = TypeAliasType("TrueLiteral", Literal[True])
LiteralText = TypeAliasType("LiteralText", LiteralString)

def f(x: Minus[Boolean, TrueLiteral], y: Minus[LiteralText, AlwaysTruthy]):
    assert_type(x, Literal[False])
    assert_type(y, Literal[""])
    reveal_type(x)  # revealed: Literal[False]
    reveal_type(y)  # revealed: Literal[""]
```

Enum complements also recognize aliases for both the enum and its excluded members.

```py
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

Palette = TypeAliasType("Palette", Color)
Red = TypeAliasType("Red", Literal[Color.RED])
Green = TypeAliasType("Green", Literal[Color.GREEN])

def colors(x: Minus[Palette, Red], y: Minus[Palette, Red | Green]):
    reveal_type(x)  # revealed: Literal[Color.GREEN, Color.BLUE]
    reveal_type(y)  # revealed: Literal[Color.BLUE]
```
