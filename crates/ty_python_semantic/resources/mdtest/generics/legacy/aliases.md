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

## Recursive calls returning a generic alias

```py
from typing import TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")
Chain = TypeAliasType("Chain", "tuple[T, Chain[T] | None]", type_params=(T,))

def chain(value: T, recurse: bool) -> Chain[T]:
    if recurse:
        _, tail = chain(1, False)
        if tail is not None:
            # revealed: Literal[1]
            reveal_type(tail[0])
    return value, (value, None)
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

## Promotion through recursive aliases

Collection inference promotes the contents of a recursive alias at every depth.

```py
from typing import TypeVar
from ty_extensions import AlwaysFalsy, Intersection, Not

class Item: ...

T = TypeVar("T")
Repeated = tuple[T, "Repeated[T]"]

def promote_recursive(value: Repeated[Intersection[Item, Not[AlwaysFalsy]]]) -> None:
    promoted = [value][0]
    reveal_type(promoted[0])  # revealed: Item
    reveal_type(promoted[1][0])  # revealed: Item
    reveal_type(promoted[1][1][0])  # revealed: Item
```

Promotion reverses direction in callable parameters, including at recursive references.

```py
from typing import Callable
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to

Narrow = Intersection[Item, Not[AlwaysFalsy]]
Source = Callable[["Source"], Narrow]
Promoted = Callable[["Unpromoted"], Item]
Unpromoted = Callable[[Promoted], Narrow]

def promote_callable(value: Source) -> None:
    promoted = [value][0]
    static_assert(is_equivalent_to(TypeOf[promoted], Promoted))
```

Materialization and promotion both reach every recursive layer. Promotion can introduce `Unknown`,
so reversing the order changes the result.

```py
from typing import Any, cast
from ty_extensions import Top

Gradual = tuple[Any, None, "Gradual"]

def materialize_then_promote(value: Top[Gradual]) -> None:
    promoted = [value][0]
    reveal_type(promoted[0])  # revealed: object
    reveal_type(promoted[1])  # revealed: None | Unknown
    reveal_type(promoted[2][2][0])  # revealed: object
    reveal_type(promoted[2][2][1])  # revealed: None | Unknown

def promote_then_materialize(value: Gradual) -> None:
    promoted = [value][0]
    top = cast(Top[TypeOf[promoted]], promoted)
    reveal_type(top[0])  # revealed: object
    reveal_type(top[1])  # revealed: object
    reveal_type(top[2][2][0])  # revealed: object
    reveal_type(top[2][2][1])  # revealed: object
```

## Singleton promotion with growing arguments

Each recursive step adds a list around the type argument. Singleton promotion follows the variance
of those lists, just as it does in the corresponding finite tuple.

```py
from typing import TypeVar

T = TypeVar("T")
Growing = tuple[T, "Growing[list[T]]"]

def promote_growing(value: Growing[None]) -> None:
    promoted = [value][0]
    reveal_type(promoted[0])  # revealed: None | Unknown
    reveal_type(promoted[1][0])  # revealed: list[None]
    reveal_type(promoted[1][1][0])  # revealed: list[list[None | Unknown]]
    reveal_type(promoted[1][1][1][0])  # revealed: list[list[list[None]]]
```

## Specializing promoted recursive aliases

Specialization after promotion does not promote the newly substituted argument. A fixed `None`
element is promoted, while an element that becomes `None` only after specialization is unchanged.
This agrees with the corresponding finite tuple.

```py
from typing import Any, TypeVar, cast
from ty_extensions._internal import TypeOf

T = TypeVar("T")
Repeated = tuple[T, None, "Repeated[T]"]
Promoted = TypeOf[[cast(Repeated[T], cast(Any, None))][0]]
FinitePromoted = TypeOf[[cast(tuple[T, None], cast(Any, None))][0]]

def specialize(recursive: Promoted[None], finite: FinitePromoted[None]) -> None:
    reveal_type(finite[0])  # revealed: None
    reveal_type(finite[1])  # revealed: None | Unknown
    reveal_type(recursive[0])  # revealed: None
    reveal_type(recursive[1])  # revealed: None | Unknown
    reveal_type(recursive[2][2][0])  # revealed: None
    reveal_type(recursive[2][2][1])  # revealed: None | Unknown
```

## Materialized specialization of recursive aliases

Reading a method from a materialized generic class substitutes its type arguments with the polarity
of each occurrence. Recursive return types follow the same rules as finite tuples, including
invariant lists introduced by later recursive steps.

```py
from typing import Any, Callable, Generic, TypeVar
from ty_extensions import Top

T = TypeVar("T")
Pairs = tuple[T, Callable[[T], T], "Pairs[T]"]
Growing = tuple[T, "Growing[list[T]]"]

class Box(Generic[T]):
    value: T

    def finite(self) -> tuple[T, Callable[[T], T], list[T], list[list[T]]]:
        raise NotImplementedError

    def pairs(self) -> Pairs[T]:
        raise NotImplementedError

    def growing(self) -> Growing[T]:
        raise NotImplementedError

def read(value: Top[Box[Any]]) -> None:
    # revealed: tuple[object, (Never, /) -> object, Top[list[Any]], Top[list[list[Any]]]]
    reveal_type(value.finite())
    reveal_type(value.pairs()[0])  # revealed: object
    reveal_type(value.pairs()[1])  # revealed: (Never, /) -> object
    reveal_type(value.pairs()[2][2][0])  # revealed: object
    reveal_type(value.pairs()[2][2][1])  # revealed: (Never, /) -> object
    reveal_type(value.growing()[0])  # revealed: object
    reveal_type(value.growing()[1][0])  # revealed: Top[list[Any]]
    reveal_type(value.growing()[1][1][0])  # revealed: Top[list[list[Any]]]
```

## Repeated materialization and specialization

Each specialization introduces another tuple containing `Any`. Materializing the result removes
those dynamic elements at every recursive depth and preserves the original type argument.

```py
from typing import Any, TypeVar
from ty_extensions import Top

T = TypeVar("T")
Repeated = tuple[T, Any, "Repeated[T]"]
Level0 = Top[Repeated[T]]
Level1 = Top[Level0[tuple[T, Any]]]
Level2 = Top[Level1[tuple[T, Any]]]
Level3 = Top[Level2[tuple[T, Any]]]
Level4 = Top[Level3[tuple[T, Any]]]
Level5 = Top[Level4[tuple[T, Any]]]
Level6 = Top[Level5[tuple[T, Any]]]
Level7 = Top[Level6[tuple[T, Any]]]
Level8 = Top[Level7[tuple[T, Any]]]
Level9 = Top[Level8[tuple[T, Any]]]
Level10 = Top[Level9[tuple[T, Any]]]
Level11 = Top[Level10[tuple[T, Any]]]

def read(value: Level11[int]) -> None:
    reveal_type(value[1])  # revealed: object
    reveal_type(value[0][1])  # revealed: object
    reveal_type(value[0][0][0][0][0][0][0][0][0][0][0][0])  # revealed: int
    reveal_type(value[2][2][0][1])  # revealed: object
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

## Capturing an enclosing type parameter

An alias defined inside a generic function refers to that function's type parameter.

```py
from typing import TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")

def outer(value: T) -> T:
    Captured = TypeAliasType("Captured", T)

    def identity(other: Captured) -> Captured:
        return other

    return identity(value)  # ok
```

The alias can also have its own parameters. Specializing them preserves the captured parameter.

```py
U = TypeVar("U")

def generic_outer(value: T) -> tuple[T, int]:
    Pair = TypeAliasType("Pair", tuple[T, U], type_params=(U,))

    def integers(other: Pair[int]) -> Pair[int]:
        return other

    def strings(other: Pair[str]) -> Pair[str]:
        return other

    strings((value, "text"))  # ok
    integers((value, "text"))  # error: [invalid-argument-type]
    return integers((value, 1))  # ok
```

## Constructor inference through captured alias parameters

A class-local alias captures its class's type parameter. Constructing another instance inside the
class infers a new specialization without constraining the enclosing instance's type parameter.

```py
from typing import Generic, TypeVar
from typing_extensions import TypeAliasType

T = TypeVar("T")

class Box(Generic[T]):
    Items = TypeAliasType("Items", list[T])

    def __init__(self, value: Items) -> None: ...
    def copy(self, value: list[T]) -> None:
        # revealed: Box[T@Box]
        reveal_type(Box(value))
        # revealed: Box[int]
        reveal_type(Box([1]))
```

The alias's own parameters retain their specialization while its captured class parameter is
inferred for the new instance.

```py
U = TypeVar("U")

class PairBox(Generic[T]):
    Pair = TypeAliasType("Pair", tuple[T, U], type_params=(U,))

    def __init__(self, value: Pair[int]) -> None: ...
    def copy(self, value: T) -> None:
        # revealed: PairBox[T@PairBox]
        reveal_type(PairBox((value, 1)))
        # revealed: PairBox[str]
        reveal_type(PairBox(("text", 1)))
        PairBox((value, "wrong"))  # error: [invalid-argument-type]
```
