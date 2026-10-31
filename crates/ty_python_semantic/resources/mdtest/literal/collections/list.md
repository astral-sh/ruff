# Lists

## Empty list

```py
reveal_type([])  # revealed: list[Unknown]
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
reveal_type([1, "a"])  # revealed: list[int | str]
# revealed: list[int | tuple[int, ...]]
reveal_type([1, (1, 2), (1, 2, 3)])
```

## Gradual list inference

Disabling strict list inference uses `Unknown` for heterogeneous elements. Homogeneous lists and the
other collection types retain their usual inferred types.

```toml
[analysis]
strict-list-inference = false
```

```py
reveal_type([1, "a"])  # revealed: list[Unknown]
reveal_type(["a", 1])  # revealed: list[Unknown]
reveal_type([1, 2])  # revealed: list[int]
reveal_type([])  # revealed: list[Unknown]
reveal_type([None])  # revealed: list[None | Unknown]
reveal_type([[1], ["a"]])  # revealed: list[Unknown]
reveal_type({1, "a"})  # revealed: set[int | str]
reveal_type({"a": 1, "b": "two"})  # revealed: dict[str, int | str]
```

The comparison uses individual element types, so a single union-typed element retains its type.
Unpacked elements participate in the comparison.

```py
def unions(value: int | str, values: list[int | str]):
    reveal_type([value])  # revealed: list[int | str]
    reveal_type([value, value])  # revealed: list[int | str]
    reveal_type([*values])  # revealed: list[int | str]
    reveal_type([*values, 1])  # revealed: list[Unknown]
    reveal_type([item for item in values])  # revealed: list[int | str]

reveal_type([*[1, 2], "a"])  # revealed: list[Unknown]
```

Different types remain heterogeneous even when their union would simplify to a common superclass.

```py
class Base: ...
class Derived(Base): ...

reveal_type([Base(), Derived()])  # revealed: list[Unknown]
```

Expected types from annotations and calls still constrain the elements and report incompatible
values. Constructor calls continue to infer their type arguments from their signatures.

```py
items: list[int | str] = [1, "a"]
reveal_type(items)  # revealed: list[int | str]
invalid: list[int] = [1, "a"]  # error: [invalid-assignment]

def accepts_ints(items: list[int]) -> None: ...

accepts_ints([1, "a"])  # error: [invalid-argument-type]
reveal_type(list((1, "a")))  # revealed: list[int | str]
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
