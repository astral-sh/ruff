# Generic type aliases: legacy syntax

## Ambiguous overload returns through generic aliases

Aliases are expanded when comparing the structure of ambiguous return types. Recursive aliases
retain their outer structure, but recursive references fall back to `Unknown`, even when each
reference changes the alias's type arguments.

```py
from typing import Any, TypeVar, overload

T = TypeVar("T")
Wrapped = dict[str, list[T]]
Growing = tuple[T, list["Growing[list[T]]"]]

@overload
def wrapped(value: int) -> Wrapped[int]: ...
@overload
def wrapped(value: str) -> Wrapped[str]: ...
def wrapped(value): ...
@overload
def growing(value: int) -> Growing[int]: ...
@overload
def growing(value: str) -> Growing[str]: ...
def growing(value): ...
def _(value: Any):
    reveal_type(wrapped(value))  # revealed: dict[str, list[Unknown]]
    reveal_type(growing(value))  # revealed: tuple[Unknown, list[Unknown]]
```

## Growing aliases inside unions

Comparing these specializations terminates even though each recursive reference adds another `list`
to its argument. We conservatively reject the return when the same recursive structure repeats.

```py
from typing import TypeVar

T = TypeVar("T")
Nested = list["Nested[list[T]] | list[Nested[list[T]]]"]

def convert(value: Nested[int]) -> Nested[str]:
    # error: [invalid-return-type]
    return value
```

## Growing aliases inside callable signatures

The same comparison can recur through a callable's return type. We stop when its recursive structure
repeats, even though the type arguments keep changing.

```py
from typing import Callable, TypeVar

T = TypeVar("T")
Producer = Callable[[], "Producer[list[T]] | Callable[[], Producer[list[T]]]"]

def convert_producer(value: Producer[int]) -> Producer[str]:
    # error: [invalid-return-type]
    return value
```

Recursive references in parameter types are handled the same way.

```py
Consumer = Callable[["Consumer[list[T]] | Callable[[Consumer[list[T]]], None]"], None]

def convert_consumer(value: Consumer[int]) -> Consumer[str]:
    # error: [invalid-return-type]
    return value
```

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

## Comparing recursive aliases inside negations

Each recursive reference below wraps `T` in another `list`. We can compare these types without
expanding every reference. The tuple's second element prevents `Nested[int]` from being assigned to
`Nested[str]`.

```py
from typing import TypeVar
from ty_extensions import Not

T = TypeVar("T")
Nested = tuple[list[Not["Nested[list[T]]"]], T]

def convert(value: Nested[int]) -> Nested[str]:
    # error: [invalid-return-type]
    return value
```

## Circular aliases through negation

Negation does not make a circular alias valid. The reference to `A` is outside the `list` used as
its type argument. Resolving the alias in an annotation reports its circular definition.

```py
from typing import TypeVar
from ty_extensions import Not

T = TypeVar("T")
# error: [cyclic-type-alias-definition]
A = T | Not["A[list[T]]"]

def use(value: A[int]) -> None: ...
```

The same applies when an intersection combines positive and negative references with different type
arguments. After reporting the circular definition, we can still check assignments involving the
alias.

```py
from ty_extensions import Intersection

# error: [cyclic-type-alias-definition]
Both = Intersection["Both[list[T]]", Not["Both[tuple[T]]"]]

def convert(value: Both[int]) -> Both[str]:
    return value  # no diagnostic
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

We use the element type of `Iterable` when simplifying its intersection with a tuple. That
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
