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

## Comparing growing recursive aliases

The type arguments of these aliases keep growing, but their comparisons terminate. The two aliases
are equivalent; proving this equivalence is not yet supported.

```py
from typing import Any, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_disjoint_from, is_equivalent_to

A = TypeVar("A")
B = TypeVar("B")
C = TypeVar("C")
Delayed = tuple[A, "Delayed[B, C, tuple[A, Any]] | None"]
Delayed2 = tuple[A, "Delayed2[B, C, tuple[A, Any]] | None"]

# TODO: Prove equivalence between aliases with matching growing arguments.
# error: [static-assert-error]
static_assert(is_equivalent_to(Delayed[int, int, int], Delayed2[int, int, int]))
static_assert(not is_disjoint_from(Delayed[int, int, int], Delayed2[int, int, int]))
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

## Mappings of growing recursive aliases

Materialization and singleton promotion reach payloads introduced by later recursive
specializations, even when the type arguments keep growing.

```py
from typing import Any, Callable, TypeVar
from ty_extensions import Bottom, Top, static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to, is_subtype_of

A = TypeVar("A")
B = TypeVar("B")
C = TypeVar("C")
Delayed = tuple[A, "Delayed[B, C, tuple[A, Any]] | None"]
Shared = tuple["Shared[B, list[A]] | None", list[A]]

static_assert(not is_equivalent_to(Delayed[int, int, int], Top[Delayed[int, int, int]]))
static_assert(not is_equivalent_to(Delayed[int, int, int], Bottom[Delayed[int, int, int]]))
static_assert(not is_subtype_of(Shared[int, Any], Shared[int, Any]))

Nested = tuple[list[list["Nested[list[A]]"]]]

def materialize_nested(value: Top[Nested[int]]): ...  # no diagnostic

# Collection inference preserves a materialized tree with no literals.
Tree = A | list["Tree[list[A]]"]

def collect_tree(value: Bottom[Tree[Any]]):
    static_assert(is_equivalent_to(TypeOf[[value][0]], Bottom[Tree[Any]]))

Late = tuple[A, "Late[B, tuple[A, None]]"]

def collect(value: Late[int, int]):
    widened = [value][0]
    reveal_type(widened[1][1][1][1][0])  # revealed: tuple[tuple[int, None | Unknown], None | Unknown]

Left = tuple[A, Callable[[], "Right[tuple[A]]"]]
Right = tuple[A, Callable[[], "Left[tuple[A]]"], Callable[[], "Right[tuple[A]]"]]

# Static arguments remain static through both recursive definitions.
static_assert(is_equivalent_to(Left[int], Top[Left[int]]))
static_assert(is_equivalent_to(Left[int], Bottom[Left[int]]))

# Materialization still applies when a later specialization introduces Any.
MaterializedLeft = Top[Left[B]]

def materialize_mutual(static: Top[Left[int]], gradual: MaterializedLeft[Any]):
    reveal_type((static, gradual[0]))  # revealed: tuple[Left[int], object]

def collect_mutual(value: Left[TypeOf[1]], flag: bool):
    widened = [value][0]
    while flag:
        widened = [widened][0]
    reveal_type(widened[1]()[1]()[0])  # revealed: tuple[tuple[int]]
    reveal_type(widened[1]()[2]()[1]()[0])  # revealed: tuple[tuple[tuple[int]]]

def transform(value: Any, /) -> Any:
    return value

Materialized = tuple[A, "Top[Materialized[list[A]]] | None", Any, TypeOf[transform], list[Any], TypeOf[list]]

def collect_materialized(value: Materialized[TypeOf[1]], flag: bool):
    widened = [value][0]
    while flag:
        widened = [widened][0]
    reveal_type(widened[0])  # revealed: int
    reveal_type(widened[2])  # revealed: Any
    next = widened[1]
    if next is not None:
        reveal_type(next[0])  # revealed: list[Literal[1]]
        reveal_type(next[2])  # revealed: object
        reveal_type(next[3])  # revealed: (value: Never, /) -> object
        reveal_type(next[4])  # revealed: Top[list[Any]]
        reveal_type(next[5])  # revealed: <class 'list'>
        tail = next[1]
        if tail is not None:
            reveal_type(tail[0])  # revealed: list[list[int]]
            reveal_type(tail[2])  # revealed: object

U = TypeVar("U", bound=int)
V = TypeVar("V", int, str)

def collect_parameters(value: Materialized[tuple[U, V]]):
    widened = [value][0]
    reveal_type(widened[0])  # revealed: tuple[U@collect_parameters, V@collect_parameters]
```
