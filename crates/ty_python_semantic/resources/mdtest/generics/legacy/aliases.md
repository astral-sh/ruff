# Generic type aliases: legacy syntax

## Negation after specializing a recursive alias

Substituting `object` for `T` makes `A[T]` equal to `object`, even though its definition is
recursive. Its negation is therefore `Never`.

```py
from typing import TypeVar
from typing_extensions import Never, assert_type
from ty_extensions import Not

T = TypeVar("T")
A = T | list["A[T]"]

def f(x: Not[A[object]]):
    assert_type(x, Never)
```

Specialization can also expose two negations. They cancel, leaving the specialized list or `int`.

```py
B = Not[list["B[T]"] | T]

def g(x: Not[B[int]]):
    assert_type(x, list[B[int]] | int)
```

## Excluding a recursive alias from a list

`Without[A]` describes a list that is not an `A`. Since `list[Any]` is gradual, its `Any` can stand
for a different type in each union member. The exclusion still matters: the element type does not
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

## Excluding a recursive alias from `object`

Here, specializing `Without` with `object` leaves just `Not[A]`. The alias `A` contains no gradual
types, so `Without[object, A] | A` covers every object.

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

## Recursive tuple elements in a protocol

Each element of `A` must be both another `A` and an `Items`. We can inspect an element without
expanding the tuple indefinitely, and an integer still fails that requirement.

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
    assert_type(x[0], WithItems[A])
    y: int = x[0]  # error: [invalid-assignment]
```

## Recursive tuple elements in `Iterable`

Ty can use the element type of `Iterable` when simplifying its intersection with a tuple. That
simplification also preserves a recursive element type.

```py
from collections.abc import Iterable
from typing import Any, TypeVar
from typing_extensions import TypeAliasType, assert_type
from ty_extensions import Intersection

T = TypeVar("T")
WithIterable = TypeAliasType("WithIterable", Intersection[Iterable[Any], T], type_params=(T,))
A = TypeAliasType("A", "tuple[WithIterable[A], ...]")

def f(x: A):
    assert_type(x[0], WithIterable[A])
```

## Boolean aliases after substitution

Subtracting `True` from `bool` leaves only `False`. Using aliases for the arguments to `Minus` does
not prevent that simplification.

```py
from typing import Literal, TypeVar
from typing_extensions import TypeAliasType
from ty_extensions import Intersection, Not

T = TypeVar("T")
U = TypeVar("U")
Minus = TypeAliasType("Minus", Intersection[T, Not[U]], type_params=(T, U))
Boolean = TypeAliasType("Boolean", bool)
TrueLiteral = TypeAliasType("TrueLiteral", Literal[True])

def f(x: Minus[Boolean, TrueLiteral]):
    reveal_type(x)  # revealed: Literal[False]
```
