# Literal promotion

```toml
[environment]
python-version = "3.12"
```

There are certain places where we promote literals to their common supertype.

We also promote `float` to `int | float` and `complex` to `int | float | complex`, even when not in
a type annotation.

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
reveal_type([1, 2, 3])  # revealed: list[Unknown | int]
reveal_type({"a": 1, "b": 2, "c": 3})  # revealed: dict[Unknown | str, Unknown | int]
reveal_type({"a", "b", "c"})  # revealed: set[Unknown | str]
```

Covariant collection literals are not promoted:

```py
reveal_type((1, 2, 3))  # revealed: tuple[Literal[1], Literal[2], Literal[3]]
reveal_type(frozenset((1, 2, 3)))  # revealed: frozenset[Literal[1, 2, 3]]
```

## Invariant and contravariant return types are promoted

Literals are promoted if they are in non-covariant position in the return type of a generic
function, or constructor of a generic class:

```py
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
```

## Literals are promoted recursively

```py
from typing import Literal

def promote[T](x: T) -> list[T]:
    return [x]

x1 = ((((1),),),)
reveal_type(x1)  # revealed: tuple[tuple[tuple[Literal[1]]]]
reveal_type(promote(x1))  # revealed: list[tuple[tuple[tuple[int]]]]

x2 = ([1, 2], [(3,), (4,)], ["5", "6"])
reveal_type(x2)  # revealed: tuple[list[Unknown | int], list[Unknown | tuple[int]], list[Unknown | str]]
```

However, this promotion should not take place if the literal type appears in contravariant position:

```py
def in_negated_position(non_zero_number: int):
    if non_zero_number == 0:
        raise ValueError()

    reveal_type(non_zero_number)  # revealed: int & ~Literal[0]
    reveal_type([non_zero_number])  # revealed: list[Unknown | (int & ~Literal[0])]
```

## Literal annotations are respected

Literal types that are explicitly annotated will not be promoted, even if they are initially
declared in a promotable position:

```py
from enum import Enum
from typing import Sequence, Literal, LiteralString

class Color(Enum):
    RED = "red"

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
reveal_type(x6)  # revealed: dict[list[Literal[1]], list[Color]]

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

Literal promotion should not apply to constrained TypeVars, since the inferred type is already one
of the constraints. Promoting it would produce a type that doesn't match any constraint.

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
from typing import Literal

def promote[T](x: T) -> list[T]:
    return [x]

x1 = "hello"
reveal_type(x1)  # revealed: Literal["hello"]
reveal_type([x1])  # revealed: list[Unknown | str]

x2: Literal["hello"] = "hello"
reveal_type(x2)  # revealed: Literal["hello"]
reveal_type([x2])  # revealed: list[Unknown | Literal["hello"]]

x3: tuple[Literal["hello"]] = ("hello",)
reveal_type(x3)  # revealed: tuple[Literal["hello"]]
reveal_type([x3])  # revealed: list[Unknown | tuple[Literal["hello"]]]

def f() -> Literal["hello"]:
    return "hello"

def id[T](x: T) -> T:
    return x

reveal_type(f())  # revealed: Literal["hello"]
reveal_type((f(),))  # revealed: tuple[Literal["hello"]]
reveal_type([f()])  # revealed: list[Unknown | Literal["hello"]]
reveal_type([id(f())])  # revealed: list[Unknown | Literal["hello"]]

def _(x: tuple[Literal["hello"]]):
    reveal_type(x)  # revealed: tuple[Literal["hello"]]
    reveal_type([x])  # revealed: list[Unknown | tuple[Literal["hello"]]]

type X = Literal["hello"]

x4: X = "hello"
reveal_type(x4)  # revealed: Literal["hello"]
reveal_type([x4])  # revealed: list[Unknown | Literal["hello"]]
```

Literal promotability is respected by unions:

```py
from typing import Literal

def _(flag: bool):
    promotable1 = "age"
    unpromotable1: Literal["age"] | None = "age" if flag else None

    reveal_type(unpromotable1 or promotable1)  # revealed: Literal["age"]
    reveal_type([unpromotable1 or promotable1])  # revealed: list[Unknown | Literal["age"]]

    promotable2 = "age" if flag else None
    unpromotable2: Literal["age"] = "age"

    reveal_type(promotable2 or unpromotable2)  # revealed: Literal["age"]
    reveal_type([promotable2 or unpromotable2])  # revealed: list[Unknown | Literal["age"]]

    promotable3 = True
    unpromotable3: Literal[True] | None = True if flag else None

    reveal_type(unpromotable3 or promotable3)  # revealed: Literal[True]
    reveal_type([unpromotable3 or promotable3])  # revealed: list[Unknown | Literal[True]]

    promotable4 = True if flag else None
    unpromotable4: Literal[True] = True

    reveal_type(promotable4 or unpromotable4)  # revealed: Literal[True]
    reveal_type([promotable4 or unpromotable4])  # revealed: list[Unknown | Literal[True]]

type X = Literal[b"bar"]

def _(x1: X | None, x2: X):
    reveal_type([x1, x2])  # revealed: list[Unknown | Literal[b"bar"] | None]
    reveal_type([x1 or x2])  # revealed: list[Unknown | Literal[b"bar"]]
```
