# Literal promotion

```toml
[environment]
python-version = "3.12"
```

There are certain places where we promote literals to their common supertype.

We also promote `float` to `int | float` and `complex` to `int | float | complex`, even when not in
a type annotation.

## All literal types are promotable

```py
from enum import Enum
from typing import Literal, LiteralString

class MyEnum(Enum):
    A = 1

def promote[T](x: T) -> list[T]:
    return [x]

def _(
    lit1: Literal["x"],
    lit2: LiteralString,
    lit3: Literal[True],
    lit4: Literal[b"x"],
    lit5: Literal[MyEnum.A],
):
    reveal_type(promote(lit1))  # revealed: list[str]
    reveal_type(promote(lit2))  # revealed: list[str]
    reveal_type(promote(lit3))  # revealed: list[bool]
    reveal_type(promote(lit4))  # revealed: list[bytes]
    reveal_type(promote(lit5))  # revealed: list[MyEnum]

reveal_type(promote(3.14))  # revealed: list[int | float]
reveal_type(promote(3.14j))  # revealed: list[int | float | complex]
```

Function types are also promoted to their `Callable` form:

```py
def lit6(_: int) -> int:
    return 0

reveal_type(promote(lit6))  # revealed: list[(_: int) -> int]
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
    def push(self, value: T) -> None:
        pass

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

## Invariant and contravariant literal arguments are respected

If a literal type is present in non-covariant position in the return type, but also in non-covariant
position in an argument type, we respect the explicitly annotated argument, and avoid promotion:

```py
from typing import Literal

class Covariant[T]:
    def pop(self) -> T:
        raise NotImplementedError

class Contravariant[T]:
    def push(self, value: T) -> None:
        pass

class Invariant[T]:
    x: T

def f1[T](x: T) -> Invariant[T] | None: ...
def f2[T](x: Covariant[T]) -> Invariant[T] | None: ...
def f3[T](x: Invariant[T]) -> Invariant[T] | None: ...
def f4[T](x: Contravariant[T]) -> Invariant[T] | None: ...
def f5[T](x: Covariant[Invariant[T]]) -> Invariant[T] | None: ...
def f6[T](x: Covariant[Invariant[T]]) -> Invariant[T] | None: ...
def f7[T](x: Covariant[T], y: Invariant[T]) -> Invariant[T] | None: ...
def f8[T](x: Invariant[T], y: Covariant[T]) -> Invariant[T] | None: ...
def f9[T](x: Covariant[T], y: Contravariant[T]) -> Invariant[T] | None: ...
def f10[T](x: Contravariant[T], y: Covariant[T]) -> Invariant[T] | None: ...
def _(
    lit: Literal[1],
    cov: Covariant[Literal[1]],
    inv: Invariant[Literal[1]],
    cont: Contravariant[Literal[1]],
    inv2: Covariant[Invariant[Literal[1]]],
):
    reveal_type(f1(lit))  # revealed: Invariant[int] | None
    reveal_type(f2(cov))  # revealed: Invariant[int] | None

    reveal_type(f3(inv))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f4(cont))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f5(inv2))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f6(inv2))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f7(cov, inv))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f8(inv, cov))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f9(cov, cont))  # revealed: Invariant[Literal[1]] | None
    reveal_type(f10(cont, cov))  # revealed: Invariant[Literal[1]] | None
```

Note that we consider variance of _argument_ types, not parameters. If the literal is in covariant
position in the declared parameter type, but invariant in the argument type, we still avoid
promotion:

```py
from typing import Iterable

class X[T]:
    def __init__(self, x: Iterable[T]): ...

def _(x: list[Literal[1]]):
    reveal_type(X(x))  # revealed: X[Literal[1]]
```

## Literals are promoted recursively

```py
from typing import Literal

def promote[T](x: T) -> list[T]:
    return [x]

def _(x: tuple[tuple[tuple[Literal[1]]]]):
    reveal_type(promote(x))  # revealed: list[tuple[tuple[tuple[int]]]]

x1 = ([1, 2], [(3,), (4,)], ["5", "6"])
reveal_type(x1)  # revealed: tuple[list[Unknown | int], list[Unknown | tuple[int]], list[Unknown | str]]
```

However, this promotion should not take place if the literal type appears in contravariant position:

```py
from typing import Callable, Literal

def in_negated_position(non_zero_number: int):
    if non_zero_number == 0:
        raise ValueError()

    reveal_type(non_zero_number)  # revealed: int & ~Literal[0]

    reveal_type([non_zero_number])  # revealed: list[Unknown | (int & ~Literal[0])]

def in_parameter_position(callback: Callable[[Literal[1]], None]):
    reveal_type(callback)  # revealed: (Literal[1], /) -> None

    reveal_type([callback])  # revealed: list[Unknown | ((Literal[1], /) -> None)]

def double_negation(callback: Callable[[Callable[[Literal[1]], None]], None]):
    reveal_type(callback)  # revealed: ((Literal[1], /) -> None, /) -> None

    reveal_type([callback])  # revealed: list[Unknown | (((int, /) -> None, /) -> None)]
```

Literal promotion should also not apply recursively to type arguments in contravariant/invariant
position:

```py
class Bivariant[T]:
    pass

class Covariant[T]:
    def pop(self) -> T:
        raise NotImplementedError

class Contravariant[T]:
    def push(self, value: T) -> None:
        pass

class Invariant[T]:
    x: T

def _(
    bivariant: Bivariant[Literal[1]],
    covariant: Covariant[Literal[1]],
    contravariant: Contravariant[Literal[1]],
    invariant: Invariant[Literal[1]],
):
    reveal_type([bivariant])  # revealed: list[Unknown | Bivariant[int]]
    reveal_type([covariant])  # revealed: list[Unknown | Covariant[int]]

    reveal_type([contravariant])  # revealed: list[Unknown | Contravariant[Literal[1]]]
    reveal_type([invariant])  # revealed: list[Unknown | Invariant[Literal[1]]]
```

## Literal annnotations are respected

Explicitly annotated `Literal` types will prevent literal promotion:

```py
from enum import Enum
from typing_extensions import Literal, LiteralString

class Color(Enum):
    RED = "red"

type Y[T] = list[T]

class X[T]:
    value: T

    def __init__(self, value: T): ...

def x[T](x: T) -> X[T]:
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

x7: X[Literal[1]] = X(1)
reveal_type(x7)  # revealed: X[Literal[1]]

x8: X[int] = X(1)
reveal_type(x8)  # revealed: X[int]

x9: dict[list[X[Literal[1]]], set[Literal[b"a"]]] = {[X(1)]: {b"a"}}
reveal_type(x9)  # revealed: dict[list[X[Literal[1]]], set[Literal[b"a"]]]

x10: list[Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(x10)  # revealed: list[Literal[1, 2, 3]]

x11: list[Literal[1] | Literal[2] | Literal[3]] = [1, 2, 3]
reveal_type(x11)  # revealed: list[Literal[1, 2, 3]]

x12: Y[Y[Literal[1]]] = [[1]]
reveal_type(x12)  # revealed: list[list[Literal[1]]]

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

x21: X[Literal[1]] | None = X(1)
reveal_type(x21)  # revealed: X[Literal[1]]

x22: X[Literal[1]] | None = x(1)
reveal_type(x22)  # revealed: X[Literal[1]]
```
