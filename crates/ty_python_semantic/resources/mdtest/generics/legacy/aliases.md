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

## Parametric relations between growing recursive aliases

Recursive aliases with growing arguments can be compared through the requirements that their
definitions impose on those arguments. Covariant growth preserves subtyping; invariant growth
requires equivalent arguments.

```py
from typing import Any, TypeVar, Union
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to, is_subtype_of

T = TypeVar("T")
A = TypeVar("A")
B = TypeVar("B")
X = TypeVar("X")
Y = TypeVar("Y")

Left = Union[tuple[T, "Left[list[T]]"], None]
Right = Union[tuple[T, "Right[list[T]]"], None]

static_assert(is_equivalent_to(Left[int], Right[int]))
static_assert(not is_subtype_of(Left[bool], Right[int]))

CovariantLeft = Union[tuple[T, "CovariantLeft[tuple[T]]"], None]
CovariantRight = Union[tuple[T, "CovariantRight[tuple[T]]"], None]

static_assert(is_subtype_of(CovariantLeft[bool], CovariantRight[int]))
static_assert(not is_subtype_of(CovariantLeft[int], CovariantRight[bool]))

PhantomLeft = Union[tuple["PhantomLeft[list[T]]"], None]
PhantomRight = Union[tuple["PhantomRight[tuple[T]]"], None]

static_assert(is_equivalent_to(PhantomLeft[int], PhantomRight[str]))
static_assert(is_equivalent_to(PhantomLeft[Any], PhantomRight[object]))
```

The correspondence between definitions includes parameter permutations, mutual recursion, and
recursive aliases used as arguments of themselves.

```py
PermutedLeft = Union[tuple[A, B, "PermutedLeft[list[A], tuple[B]]"], None]
PermutedRight = Union[tuple[Y, X, "PermutedRight[list[Y], tuple[X]]"], None]

# Legacy parameter order follows first occurrence, so Y precedes X in PermutedRight.
static_assert(is_equivalent_to(PermutedLeft[int, str], PermutedRight[int, str]))

Mutual = Union[tuple[T, "MutualNext[list[T]]"], None]
MutualNext = Union[tuple[T, "Mutual[list[T]]"], None]

OtherMutual = Union[tuple[T, "OtherMutualNext[list[T]]"], None]
OtherMutualNext = Union[tuple[T, "OtherMutual[list[T]]"], None]

static_assert(is_equivalent_to(Mutual[int], OtherMutual[int]))

# TODO: These are equivalent, but comparing one step with two inlined steps needs a
# correspondence between a type parameter on one side and list[T] on the other.
static_assert(not is_equivalent_to(Left[int], Mutual[int]))

NestedLeft = Union[tuple[T, "NestedLeft[NestedLeft[T]]"], None]
NestedRight = Union[tuple[T, "NestedRight[NestedRight[T]]"], None]

static_assert(is_equivalent_to(NestedLeft[int], NestedRight[int]))
```

Equal arguments do not imply equivalent definitions when different parameters are exposed by each
definition.

```py
ProjectedLeft = Union[tuple[A, "ProjectedLeft[list[A], tuple[B]]"], None]
ProjectedRight = Union[tuple[B, "ProjectedRight[list[A], tuple[B]]"], None]

static_assert(not is_equivalent_to(ProjectedLeft[int, int], ProjectedRight[int, int]))
```

Nominal generic classes contribute their declared or inferred variance to the recursive relation.

```py
from typing import Generic

C_co = TypeVar("C_co", covariant=True)
C_contra = TypeVar("C_contra", contravariant=True)

class Producer(Generic[C_co]):
    def read(self) -> C_co:
        raise NotImplementedError

class Consumer(Generic[C_contra]):
    def write(self, value: C_contra) -> None: ...

EndpointLeft = Union[tuple[Producer[A], Consumer[B], "EndpointLeft[tuple[A], tuple[B]]"], None]
EndpointRight = Union[tuple[Producer[A], Consumer[B], "EndpointRight[tuple[A], tuple[B]]"], None]

static_assert(is_subtype_of(EndpointLeft[bool, int], EndpointRight[int, bool]))
static_assert(not is_subtype_of(EndpointRight[int, bool], EndpointLeft[bool, int]))
```

Contravariant positions reverse the parameter requirements. Alternating positive and negative
occurrences require both directions.

```py
from typing import Callable, Literal

ConsumerLeft = Union[Callable[[T], "ConsumerLeft[tuple[T]]"], None]
ConsumerRight = Union[Callable[[T], "ConsumerRight[tuple[T]]"], None]

static_assert(is_subtype_of(ConsumerLeft[int], ConsumerRight[bool]))
static_assert(not is_subtype_of(ConsumerLeft[bool], ConsumerRight[int]))

AlternatingLeft = Union[Callable[["AlternatingLeft[tuple[T]]"], T], None]
AlternatingRight = Union[Callable[["AlternatingRight[tuple[T]]"], T], None]

static_assert(is_equivalent_to(AlternatingLeft[int], AlternatingRight[int]))
static_assert(not is_subtype_of(AlternatingLeft[bool], AlternatingRight[int]))

static_assert(is_subtype_of(CovariantLeft[Literal[1]], CovariantRight[int]))
```

Variable-length tuples can contain recursive elements as well as fixed prefixes and suffixes.

```py
TupleLeft = tuple[T, *tuple[T, ...], "TupleLeft[tuple[T]]"]
TupleRight = tuple[*tuple[T, ...], "TupleRight[tuple[T]]"]

static_assert(is_subtype_of(TupleLeft[bool], TupleRight[int]))
static_assert(not is_subtype_of(TupleRight[bool], TupleLeft[int]))
```

Union comparisons may choose different alternatives for different specializations.

```py
ChoiceLeft = Union[tuple[A, "ChoiceLeft[tuple[A], tuple[B]]"], tuple[B, "ChoiceLeft[tuple[A], tuple[B]]"]]
ChoiceRight = Union[tuple[A, "ChoiceRight[tuple[A], tuple[B]]"], tuple[B, "ChoiceRight[tuple[A], tuple[B]]"]]

static_assert(is_equivalent_to(ChoiceLeft[int, str], ChoiceRight[str, int]))
static_assert(not is_equivalent_to(ChoiceLeft[int, bytes], ChoiceRight[str, int]))
```

Choices for different tuple elements remain independent when recursion combines all their
requirements, including when those rows appear in contravariant positions.

```py
C = TypeVar("C")
D = TypeVar("D")
P = TypeVar("P")
Q = TypeVar("Q")
R = TypeVar("R")
S = TypeVar("S")
U = TypeVar("U")
V = TypeVar("V")

Rows = Union[
    tuple[
        tuple[A, A, A],
        tuple[B, B, B],
        tuple[C, C, C],
        tuple[D, D, D],
        "Rows[tuple[A], tuple[B], tuple[C], tuple[D]]",
    ],
    None,
]
ChoiceRows = Union[
    tuple[
        tuple[P | Q, R | S, U | V],
        tuple[P | Q, R | S, U | V],
        tuple[P | Q, R | S, U | V],
        tuple[P | Q, R | S, U | V],
        "ChoiceRows[tuple[P], tuple[Q], tuple[R], tuple[S], tuple[U], tuple[V]]",
    ],
    None,
]

static_assert(is_subtype_of(Rows[bool, str, str, int], ChoiceRows[int, str, str, int, int, str]))
static_assert(not is_subtype_of(Rows[bool, str, bytes, int], ChoiceRows[int, str, str, int, int, str]))

AcceptRows = Union[Callable[[Rows[A, B, C, D]], "AcceptRows[tuple[A], tuple[B], tuple[C], tuple[D]]"], None]
AcceptChoices = Union[
    Callable[[ChoiceRows[P, Q, R, S, U, V]], "AcceptChoices[tuple[P], tuple[Q], tuple[R], tuple[S], tuple[U], tuple[V]]"],
    None,
]

static_assert(is_subtype_of(AcceptChoices[int, str, str, int, int, str], AcceptRows[bool, str, str, int]))
static_assert(not is_subtype_of(AcceptChoices[int, str, str, int, int, str], AcceptRows[bool, str, bytes, int]))
```

Positive intersections combine requirements on the same type. Assignability to each component
separately is insufficient when those comparisons choose incompatible materializations of `Any`.

```py
from ty_extensions import Intersection
from ty_extensions._internal import is_assignable_to

MeetLeft = Union[tuple[Intersection[A, B], "MeetLeft[list[A], list[B]]"], None]
MeetRight = Union[tuple[Intersection[A, B], "MeetRight[list[A], list[B]]"], None]

static_assert(is_equivalent_to(MeetLeft[int, int], MeetRight[int, int]))

Source = Union[tuple[T, "Source[list[A], list[A]]"], None]
Target = Union[tuple[Intersection[T, P], "Target[list[A], list[B], list[A], list[B]]"], None]

static_assert(not is_assignable_to(Source[int, Any], Target[int, int, int, str]))

def source_value() -> Source[int, Any]:
    return (1, ([], None))

def target_value() -> Target[int, int, int, str]:
    return (1, ([], None))  # error: [invalid-return-type]
```
