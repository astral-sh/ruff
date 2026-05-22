# Type promotion

```toml
[environment]
python-version = "3.12"
```

There are certain places (usually when inferring a type for a typevar in an invariant position)
where we "promote" types to a supertype, rather than inferring the most precise possible types. For
example, we don't want `[1, 2]` to be inferred as `list[Literal[1, 2]]`, since that would prevent
adding a `3` to the list later; we prefer `list[int]` instead.

This is a heuristic, where we are trying to guess the type the user probably means, in the absence
of a clarifying annotation, and in place of trying to do global inference that accounts for every
use up-front.

In addition to promoting literal types to their nominal supertype (e.g. `Literal[1]` to `int`,
`Literal["foo"]` to `str`, we also promote `float` to `int | float` and `complex` to
`int | float | complex`.

We also remove negative intersection elements, so that e.g. `A & ~AlwaysFalsy` promotes to simply
`A`.

We avoid promoting literal types that originate from an explicit annotation.

## Implicitly inferred literal types are promotable

Any literal types that are implicitly inferred are promotable:

```py
from enum import Enum
from typing import Literal, LiteralString

class MyEnum(Enum):
    A = 1
    B = 2

def promote[T](x: T) -> list[T]:
    return [x]

x1 = "hello"
reveal_type(x1)  # revealed: Literal["hello"]
reveal_type(promote(x1))  # revealed: list[str]

x2 = True
reveal_type(x2)  # revealed: Literal[True]
reveal_type(promote(x2))  # revealed: list[bool]

x3 = b"hello"
reveal_type(x3)  # revealed: Literal[b"hello"]
reveal_type(promote(x3))  # revealed: list[bytes]

x4 = MyEnum.A
reveal_type(x4)  # revealed: Literal[MyEnum.A]
reveal_type(promote(x4))  # revealed: list[MyEnum]

x5 = 3.14
reveal_type(x5)  # revealed: float
reveal_type(promote(x5))  # revealed: list[int | float]

x6 = 3.14j
reveal_type(x6)  # revealed: complex
reveal_type(promote(x6))  # revealed: list[int | float | complex]
```

Function types are also promoted to their `Callable` form:

```py
def f(_: int) -> int:
    return 0

reveal_type(f)  # revealed: def f(_: int) -> int
reveal_type(promote(f))  # revealed: list[(_: int) -> int]
```

## Invariant collection literals are promoted

The elements of invariant collection literals, i.e. lists, dictionaries, and sets, are promoted:

```py
reveal_type([1, 2, 3])  # revealed: list[int]
reveal_type({"a": 1, "b": 2, "c": 3})  # revealed: dict[str, int]
reveal_type({"a", "b", "c"})  # revealed: set[str]
```

Covariant collection literals are not promoted:

```py
reveal_type((1, 2, 3))  # revealed: tuple[Literal[1], Literal[2], Literal[3]]
reveal_type(frozenset((1, 2, 3)))  # revealed: frozenset[Literal[1, 2, 3]]
```

## Unions of homogeneous, fixed-length tuples can be promoted to a single variadic tuple

This type of promotion applies specifically when a collection literal contains at least two tuple
literals that share an element type but that differ in length. The inferred type of those literals
is a union of homogeneous, fixed-length tuples, which is subsequently promoted to a single, variadic
tuple. The type of any non-tuple elements in the collection literal is preserved in the final
inferred type.

```py
reveal_type([(1, 2), (3, 4, 5)])  # revealed: list[tuple[int, ...]]
reveal_type({".py": (".py", ".pyi"), ".js": (".js", ".jsx", ".ts", ".tsx")})  # revealed: dict[str, tuple[str, ...]]
reveal_type({(1, 2), (3, 4, 5)})  # revealed: set[tuple[int, ...]]
reveal_type([0, (1, 2), "a", (3, 4, 5)])  # revealed: list[int | str | tuple[int, ...]]
```

We only widen unions of fixed-length tuples. A standalone tuple type retains its fixed length.

```py
def promote[T](x: T) -> list[T]:
    return [x]

reveal_type([()])  # revealed: list[tuple[()]]
reveal_type((1, 2))  # revealed: tuple[Literal[1], Literal[2]]
reveal_type(promote((1, 2)))  # revealed: list[tuple[int, int]]
```

Tuple literals of the same length also keep their fixed length. For example, a declared collection
representing coordinate pairs does not later accept a coordinate triple.

```py
coordinates = {
    "home": (0, 0),
    "palm-tree": (10, 8),
}
reveal_type(coordinates)  # revealed: dict[str, tuple[int, int]]
coordinates["treasure"] = (5, 6, -10)  # error: [invalid-assignment]
```

Heterogeneous tuples are not widened.

```py
reveal_type([(1, "a"), (2, "b")])  # revealed: list[tuple[int, str]]
reveal_type([(1, 2), ("a", "b", "c")])  # revealed: list[tuple[int, int] | tuple[str, str, str]]
```

Normal union simplification can still generalize heterogeneous tuples of the same length, but the
result should not widen to a variadic tuple.

```py
mixed_tuples = [
    (1, "a"),
    (object(), object()),
    (object(), object(), object()),
]
reveal_type(mixed_tuples)  # revealed: list[tuple[object, object] | tuple[object, object, object]]
```

Empty tuples are treated specially: they do not count towards the minimum of two tuples that differ
in length. This allows us to preserve accurate types for collections that model exactly two possible
tuple shapes (empty or length N).

```py
reveal_type([(), (1,)])  # revealed: list[tuple[()] | tuple[int]]
reveal_type([(), (1,), (2,)])  # revealed: list[tuple[()] | tuple[int]]
```

However, an empty tuple can be subsumed by a widened tuple type when enough evidence exists
independently of the empty tuple.

```py
reveal_type([(), (1, 2), (1, 2, 3)])  # revealed: list[tuple[int, ...]]
```

A union of tuples is not widened when it is inferred into a collection (i.e., only unions that came
from direct tuple literals in the collection are widened).

```py
def get_padding() -> int | tuple[int] | tuple[int, int]:
    return (0, 1)

reveal_type([get_padding()])  # revealed: list[int | tuple[int] | tuple[int, int]]
```

No promotion occurs in a collection that mixes literal and non-literal tuples:

```py
def get_segment() -> tuple[int] | tuple[int, int, int, int]:
    return (0,)

def get_segments() -> list[tuple[int, int]]:
    return [(0, 1)]

def get_segments_by_name() -> dict[str, tuple[int, int]]:
    return {"origin": (0, 1)}

segments = [get_segment(), (1, 2), (3, 4, 5)]
reveal_type(segments)  # revealed: list[tuple[int] | tuple[int, int, int, int] | tuple[int, int] | tuple[int, int, int]]
segments.append((6, 7, 8, 9, 10))  # error: [invalid-argument-type]

starred_segments = [*get_segments(), (1, 2), (3, 4, 5)]
reveal_type(starred_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
starred_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

mapping_segments = {**get_segments_by_name(), "start": (1, 2), "end": (3, 4, 5)}
reveal_type(mapping_segments)  # revealed: dict[str, tuple[int, int] | tuple[int, int, int]]
mapping_segments["bad"] = (6, 7, 8, 9)  # error: [invalid-assignment]
```

This also applies when the non-literal tuple type is hidden behind a type alias or a type variable,
or when it is subsumed by one of the literal tuple types while building the union.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import NewType, TypeVar
from typing_extensions import TypeIs

type Segment = tuple[int, int]
NewTypeSegment = NewType("NewTypeSegment", tuple[int, int])
BoundSegment = TypeVar("BoundSegment", bound=tuple[int, int])
ConstrainedSegment = TypeVar("ConstrainedSegment", tuple[int, int], tuple[int, int, int])

class P: ...

def is_p(x: object) -> TypeIs[P]:
    return True

def get_aliased_segment() -> Segment:
    return (0, 1)

aliased_segments = [get_aliased_segment(), (1, 2), (3, 4, 5)]
reveal_type(aliased_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
aliased_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def get_newtype_segment() -> NewTypeSegment:
    return NewTypeSegment((0, 1))

newtype_segments = [get_newtype_segment(), (1, 2), (3, 4, 5)]
reveal_type(newtype_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
newtype_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def check_bound_typevar_segment(segment: BoundSegment) -> None:
    bound_typevar_segments = [segment, (1, 2), (3, 4, 5)]
    reveal_type(bound_typevar_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
    bound_typevar_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def check_constrained_typevar_segment(segment: ConstrainedSegment) -> None:
    constrained_typevar_segments = [segment, (1, 2), (3, 4, 5)]
    # revealed: list[ConstrainedSegment@check_constrained_typevar_segment | tuple[int, int] | tuple[int, int, int]]
    reveal_type(constrained_typevar_segments)
    constrained_typevar_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def get_subsumed_segment() -> tuple[bool, bool]:
    return (True, False)

subsumed_segments = [get_subsumed_segment(), (1, 2), (3, 4, 5)]
reveal_type(subsumed_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
subsumed_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def get_short_subsumed_segment() -> tuple[bool]:
    return (True,)

short_subsumed_segments = [get_short_subsumed_segment(), (1, 2), (3, 4, 5)]
reveal_type(short_subsumed_segments)  # revealed: list[tuple[bool] | tuple[int, int] | tuple[int, int, int]]
short_subsumed_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def get_heterogeneous_subsumed_segment() -> tuple[bool, int]:
    return (True, 0)

heterogeneous_subsumed_segments = [get_heterogeneous_subsumed_segment(), (1, 2), (3, 4, 5)]
reveal_type(heterogeneous_subsumed_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
heterogeneous_subsumed_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]

def check_intersection_segment(segment: tuple[int, int]) -> None:
    if is_p(segment):
        reveal_type(segment)  # revealed: tuple[int, int] & P
        intersection_segments = [segment, (1, 2), (3, 4, 5)]
        reveal_type(intersection_segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
        intersection_segments.append((6, 7, 8, 9))  # error: [invalid-argument-type]
```

No promotion occurs when a covariant collection type context provides a fixed-length tuple.

```py
from typing import Sequence

segments: Sequence[tuple[int, int] | tuple[int, int, int]] = [(1, 2), (3, 4, 5)]
reveal_type(segments)  # revealed: list[tuple[int, int] | tuple[int, int, int]]
```

Promotion still occurs when a covariant collection type context provides a gradual element type.

```py
from typing import Any
from collections.abc import Mapping

segments: Mapping[str, Any] = {"start": (1, 2), "end": (3, 4, 5)}
reveal_type(segments)  # revealed: dict[str, tuple[int, ...]]
```

## Invariant and contravariant return types are promoted

We promote in non-covariant position in the return type of a generic function, or constructor of a
generic class:

```py
from typing import Callable, Literal

class Bivariant[T]:
    def __init__(self, value: T): ...

class Covariant[T]:
    def __init__(self, value: T): ...
    def pop(self) -> T:
        raise NotImplementedError

class Contravariant[T]:
    def __init__(self, value: T): ...
    def push(self, value: T) -> None: ...

class Invariant[T]:
    x: T

    def __init__(self, value: T): ...

def f1[T](x: T) -> Bivariant[T] | None: ...
def f2[T](x: T) -> Covariant[T] | None: ...
def f3[T](x: T) -> Covariant[T] | Bivariant[T] | None: ...
def f4[T](x: T) -> Contravariant[T] | None: ...
def f5[T](x: T) -> Invariant[T] | None: ...
def f6[T](x: T) -> Invariant[T] | Contravariant[T] | None: ...
def f7[T](x: T) -> Covariant[T] | Contravariant[T] | None: ...
def f8[T](x: T) -> Invariant[T] | Covariant[T] | None: ...
def f9[T](x: T) -> tuple[Invariant[T], Invariant[T]] | None: ...
def f10[T, U](x: T, y: U) -> tuple[Invariant[T], Covariant[U]] | None: ...
def f11[T, U](x: T, y: U) -> tuple[Invariant[Covariant[T] | None], Covariant[U]] | None: ...
def f12[T](x: T) -> Callable[[T], bool] | None: ...
def f13[T](x: T) -> Callable[[bool], Invariant[T]] | None: ...

reveal_type(Bivariant(1))  # revealed: Bivariant[Literal[1]]
reveal_type(Covariant(1))  # revealed: Covariant[Literal[1]]

reveal_type(Contravariant(1))  # revealed: Contravariant[int]
reveal_type(Invariant(1))  # revealed: Invariant[int]

reveal_type(f1(1))  # revealed: Bivariant[Literal[1]] | None
reveal_type(f2(1))  # revealed: Covariant[Literal[1]] | None
reveal_type(f3(1))  # revealed: Covariant[Literal[1]] | Bivariant[Literal[1]] | None

reveal_type(f4(1))  # revealed: Contravariant[int] | None
reveal_type(f5(1))  # revealed: Invariant[int] | None
reveal_type(f6(1))  # revealed: Invariant[int] | Contravariant[int] | None
reveal_type(f7(1))  # revealed: Covariant[int] | Contravariant[int] | None
reveal_type(f8(1))  # revealed: Invariant[int] | Covariant[int] | None
reveal_type(f9(1))  # revealed: tuple[Invariant[int], Invariant[int]] | None

reveal_type(f10(1, 1))  # revealed: tuple[Invariant[int], Covariant[Literal[1]]] | None
reveal_type(f11(1, 1))  # revealed: tuple[Invariant[Covariant[int] | None], Covariant[Literal[1]]] | None

reveal_type(f12(1))  # revealed: ((int, /) -> bool) | None
reveal_type(f13(1))  # revealed: ((bool, /) -> Invariant[int]) | None
```

## Promotion is recursive

```py
from typing import Literal

def promote[T](x: T) -> list[T]:
    return [x]

x1 = ((((1),),),)
reveal_type(x1)  # revealed: tuple[tuple[tuple[Literal[1]]]]
reveal_type(promote(x1))  # revealed: list[tuple[tuple[tuple[int]]]]

x2 = ([1, 2], [(3,), (4,)], ["5", "6"])
reveal_type(x2)  # revealed: tuple[list[int], list[tuple[int]], list[str]]
```

However, this promotion should not take place in contravariant position:

```py
from typing import Generic, TypeVar
from ty_extensions import Intersection, Not, AlwaysFalsy

T_co = TypeVar("T_co", covariant=True)
T_contra = TypeVar("T_contra", contravariant=True)

class A: ...
class Consumer(Generic[T_contra]): ...
class Producer(Generic[T_co]): ...

def _(c: Consumer[Intersection[A, Not[AlwaysFalsy]]], p: Producer[Intersection[A, Not[AlwaysFalsy]]]):
    reveal_type(c)  # revealed: Consumer[A & ~AlwaysFalsy]
    reveal_type(p)  # revealed: Producer[A & ~AlwaysFalsy]
    reveal_type([c])  # revealed: list[Consumer[A & ~AlwaysFalsy]]
    reveal_type([p])  # revealed: list[Producer[A]]
```

## Literal annotations are respected

Literal types that are explicitly annotated will not be promoted, even if they are initially
declared in a promotable position:

```py
from enum import Enum
from typing import Sequence, Literal, LiteralString
from typing import Callable

class Color(Enum):
    RED = "red"
    BLUE = "blue"

type Y[T] = list[T]

class X[T]:
    value: T
    def __init__(self, value: list[T]): ...

def x[T](x: list[T]) -> X[T]:
    return X(x)

x1: list[Literal[1]] = [1]
reveal_type(x1)  # revealed: list[Literal[1]]

x2: list[Literal[True]] = [True]
reveal_type(x2)  # revealed: list[Literal[True]]

x3: list[Literal["a"]] = ["a"]
reveal_type(x3)  # revealed: list[Literal["a"]]

x4: list[LiteralString] = ["a", "b", "c"]
reveal_type(x4)  # revealed: list[LiteralString]

x5: list[list[Literal[1]]] = [[1]]
reveal_type(x5)  # revealed: list[list[Literal[1]]]

x6: dict[list[Literal[1]], list[Literal[Color.RED]]] = {[1]: [Color.RED, Color.RED]}
reveal_type(x6)  # revealed: dict[list[Literal[1]], list[Literal[Color.RED]]]

x7: X[Literal[1]] = X([1])
reveal_type(x7)  # revealed: X[Literal[1]]

x8: X[int] = X([1])
reveal_type(x8)  # revealed: X[int]

x9: dict[list[X[Literal[1]]], set[Literal[b"a"]]] = {[X([1])]: {b"a"}}
reveal_type(x9)  # revealed: dict[list[X[Literal[1]]], set[Literal[b"a"]]]

x10: list[Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(x10)  # revealed: list[Literal[1, 2, 3]]

x11: list[Literal[1] | Literal[2] | Literal[3]] = [1, 2, 3]
reveal_type(x11)  # revealed: list[Literal[1, 2, 3]]

x12: Y[Y[Literal[1]]] = [[1]]
reveal_type(x12)  # revealed: list[Y[Literal[1]]]

x13: list[tuple[Literal[1], Literal[2], Literal[3]]] = [(1, 2, 3)]
reveal_type(x13)  # revealed: list[tuple[Literal[1], Literal[2], Literal[3]]]

x14: list[tuple[int, str, int]] = [(1, "2", 3), (4, "5", 6)]
reveal_type(x14)  # revealed: list[tuple[int, str, int]]

x15: list[tuple[Literal[1], ...]] = [(1, 1, 1)]
reveal_type(x15)  # revealed: list[tuple[Literal[1], ...]]

x16: list[tuple[int, ...]] = [(1, 1, 1)]
reveal_type(x16)  # revealed: list[tuple[int, ...]]

x17: list[int | Literal[1]] = [1]
reveal_type(x17)  # revealed: list[int]

x18: list[Literal[1, 2, 3, 4]] = [1, 2]
reveal_type(x18)  # revealed: list[Literal[1, 2, 3, 4]]

x19: list[Literal[1]]

x19 = [1]
reveal_type(x19)  # revealed: list[Literal[1]]

(x19 := [1])
reveal_type(x19)  # revealed: list[Literal[1]]

x20: list[Literal[1]] | None = [1]
reveal_type(x20)  # revealed: list[Literal[1]]

x21: X[Literal[1]] | None = X([1])
reveal_type(x21)  # revealed: X[Literal[1]]

x22: X[Literal[1]] | None = x([1])
reveal_type(x22)  # revealed: X[Literal[1]]

def make_callable[T](x: T) -> Callable[[T], bool]:
    raise NotImplementedError

def maybe_make_callable[T](x: T) -> Callable[[T], bool] | None:
    raise NotImplementedError

x23: Callable[[Literal[1]], bool] = make_callable(1)
reveal_type(x23)  # revealed: (Literal[1], /) -> bool

x24: Callable[[Literal[1]], bool] | None = maybe_make_callable(1)
reveal_type(x24)  # revealed: ((Literal[1], /) -> bool) | None
```

## Literal annotations see through subtyping

Literal annotations are respected even if the inferred type is a subtype of the declared type:

```py
from typing import Any, Iterable, Literal, MutableSequence, Sequence

x1: Sequence[Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(x1)  # revealed: list[Literal[1, 2, 3]]

x2: MutableSequence[Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(x2)  # revealed: list[Literal[1, 2, 3]]

x3: Iterable[Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(x3)  # revealed: list[Literal[1, 2, 3]]

x4: Iterable[Literal[1, 2, 3]] = list([1, 2, 3])
reveal_type(x4)  # revealed: list[Literal[1, 2, 3]]

x5: frozenset[Literal[1]] = frozenset([1])
reveal_type(x5)  # revealed: frozenset[Literal[1]]

class Sup1[T]:
    value: T

class Sub1[T](Sup1[T]): ...

def sub1[T](value: T) -> Sub1[T]:
    return Sub1()

x6: Sub1[Literal[1]] = sub1(1)
reveal_type(x6)  # revealed: Sub1[Literal[1]]

x7: Sup1[Literal[1]] = sub1(1)
reveal_type(x7)  # revealed: Sub1[Literal[1]]

x8: Sup1[Literal[1]] | None = sub1(1)
reveal_type(x8)  # revealed: Sub1[Literal[1]]

x9: Sup1[Literal[1]] | None = sub1(1)
reveal_type(x9)  # revealed: Sub1[Literal[1]]

class Sup2A[T, U]:
    value: tuple[T, U]

class Sup2B[T, U]:
    value: tuple[T, U]

class Sub2[T, U](Sup2A[T, Any], Sup2B[Any, U]): ...

def sub2[T, U](x: T, y: U) -> Sub2[T, U]:
    return Sub2()

x10 = sub2(1, 2)
reveal_type(x10)  # revealed: Sub2[int, int]

x11: Sup2A[Literal[1], Literal[2]] = sub2(1, 2)
reveal_type(x11)  # revealed: Sub2[Literal[1], int]

x12: Sup2B[Literal[1], Literal[2]] = sub2(1, 2)
reveal_type(x12)  # revealed: Sub2[int, Literal[2]]
```

## Constrained TypeVars with Literal constraints

Promotion should not apply to constrained TypeVars, since the inferred type is already one of the
constraints. Promoting it would produce a type that doesn't match any constraint.

```py
from typing import TypeVar, Literal, Generic

TU = TypeVar("TU", Literal["ms"], Literal["us"])

def f(unit: TU) -> TU:
    return unit

reveal_type(f("us"))  # revealed: Literal["us"]
reveal_type(f("ms"))  # revealed: Literal["ms"]

class Timedelta(Generic[TU]):
    def __init__(self, epoch: int, time_unit: TU) -> None:
        self._epoch = epoch
        self._time_unit = time_unit

def convert(nanoseconds: int, time_unit: TU) -> Timedelta[TU]:
    return Timedelta[TU](nanoseconds // 1_000, time_unit)

delta0 = Timedelta[Literal["us"]](1_000, "us")
delta1 = Timedelta(1_000, "us")
delta2 = convert(1_000_000, "us")

reveal_type(delta0)  # revealed: Timedelta[Literal["us"]]
reveal_type(delta1)  # revealed: Timedelta[Literal["us"]]
reveal_type(delta2)  # revealed: Timedelta[Literal["us"]]

# Upper-bounded TypeVars with a Literal bound should also avoid promotion
# when the promoted type would violate the bound.
TB = TypeVar("TB", bound=Literal["ms", "us"])

def g(unit: TB) -> TB:
    return unit

reveal_type(g("us"))  # revealed: Literal["us"]

# Upper-bounded TypeVars in invariant return position: promotion should
# still be blocked when it would violate the bound.
def g2(unit: TB) -> list[TB]:
    return [unit]

reveal_type(g2("us"))  # revealed: list[Literal["us"]]

# But a non-Literal upper bound should still allow promotion.
TI = TypeVar("TI", bound=int)

def h(x: TI) -> list[TI]:
    return [x]

reveal_type(h(1))  # revealed: list[int]
```

## Literal annotations from declaration are respected

Literal types that are explicitly annotated when declared will not be promoted, even if they are
later used in a promotable position:

```py
from enum import Enum
from typing import Callable, Literal

def promote[T](x: T) -> list[T]:
    return [x]

x1 = "hello"
reveal_type(x1)  # revealed: Literal["hello"]
reveal_type([x1])  # revealed: list[str]

x2: Literal["hello"] = "hello"
reveal_type(x2)  # revealed: Literal["hello"]
reveal_type([x2])  # revealed: list[Literal["hello"]]

x3: tuple[Literal["hello"]] = ("hello",)
reveal_type(x3)  # revealed: tuple[Literal["hello"]]
reveal_type([x3])  # revealed: list[tuple[Literal["hello"]]]

def f() -> Literal["hello"]:
    return "hello"

def id[T](x: T) -> T:
    return x

reveal_type(f())  # revealed: Literal["hello"]
reveal_type((f(),))  # revealed: tuple[Literal["hello"]]
reveal_type([f()])  # revealed: list[Literal["hello"]]
reveal_type([id(f())])  # revealed: list[Literal["hello"]]

def _(x: tuple[Literal["hello"]]):
    reveal_type(x)  # revealed: tuple[Literal["hello"]]
    reveal_type([x])  # revealed: list[tuple[Literal["hello"]]]

type X = Literal["hello"]

x4: X = "hello"
reveal_type(x4)  # revealed: Literal["hello"]
reveal_type([x4])  # revealed: list[X]

class MyEnum(Enum):
    A = 1
    B = 2
    C = 3

def _(x: Literal[MyEnum.A, MyEnum.B]):
    reveal_type(x)  # revealed: Literal[MyEnum.A, MyEnum.B]
    reveal_type([x])  # revealed: list[Literal[MyEnum.A, MyEnum.B]]

def make_callable[T](x: T) -> Callable[[T], bool]:
    raise NotImplementedError

def maybe_make_callable[T](x: T) -> Callable[[T], bool] | None:
    raise NotImplementedError

def _(x: Literal[1]):
    reveal_type(make_callable(x))  # revealed: (Literal[1], /) -> bool
    reveal_type(maybe_make_callable(x))  # revealed: ((Literal[1], /) -> bool) | None
```

Literal promotability is respected by unions:

```py
from typing import Literal

def _(flag: bool):
    promotable1 = "age"
    unpromotable1: Literal["age"] | None = "age" if flag else None

    reveal_type(unpromotable1 or promotable1)  # revealed: Literal["age"]
    reveal_type([unpromotable1 or promotable1])  # revealed: list[Literal["age"]]

    promotable2 = "age" if flag else None
    unpromotable2: Literal["age"] = "age"

    reveal_type(promotable2 or unpromotable2)  # revealed: Literal["age"]
    reveal_type([promotable2 or unpromotable2])  # revealed: list[Literal["age"]]

    promotable3 = True
    unpromotable3: Literal[True] | None = True if flag else None

    reveal_type(unpromotable3 or promotable3)  # revealed: Literal[True]
    reveal_type([unpromotable3 or promotable3])  # revealed: list[Literal[True]]

    promotable4 = True if flag else None
    unpromotable4: Literal[True] = True

    reveal_type(promotable4 or unpromotable4)  # revealed: Literal[True]
    reveal_type([promotable4 or unpromotable4])  # revealed: list[Literal[True]]

type X = Literal[b"bar"]

def _(x1: X | None, x2: X):
    reveal_type([x1, x2])  # revealed: list[Literal[b"bar"] | None]
    reveal_type([x1 or x2])  # revealed: list[Literal[b"bar"]]
```

## Negative intersection elements are removed

Truthiness narrowing should not leak into invariant literal container inference:

```py
class A: ...

def _(a: A | None):
    if a:
        d = {"a": a}
        reveal_type(d)  # revealed: dict[str, A]
    return {}
```

## Module-literal types are not promoted

Since module-literal types are "literal" types in a certain sense (each type is a singleton type),
we used to promote module-literal types to `types.ModuleType`. We no longer do, because
`types.ModuleType` is a very broad type that is not particularly useful. The fake
`types.ModuleType.__getattr__` method that typeshed provides also meant that you would not receive
any errors from clearly incorrect code like this:

`module1.py`:

```py
```

`main.py`:

```py
import module1

my_modules = [module1]
reveal_type(my_modules)  # revealed: list[<module 'module1'>]
my_modules[0].flibbertigibbet  # error: [unresolved-attribute]
```
