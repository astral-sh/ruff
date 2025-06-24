# Tuple subscripts

## Indexing

```py
t = (1, "a", "b")

reveal_type(t[0])  # revealed: Literal[1]
reveal_type(t[1])  # revealed: Literal["a"]
reveal_type(t[-1])  # revealed: Literal["b"]
reveal_type(t[-2])  # revealed: Literal["a"]

reveal_type(t[False])  # revealed: Literal[1]
reveal_type(t[True])  # revealed: Literal["a"]

a = t[4]  # error: [index-out-of-bounds]
reveal_type(a)  # revealed: Unknown

b = t[-4]  # error: [index-out-of-bounds]
reveal_type(b)  # revealed: Unknown
```

## Slices

```py
def _(m: int, n: int):
    t = (1, "a", None, b"b")

    reveal_type(t[0:0])  # revealed: tuple[()]
    reveal_type(t[0:1])  # revealed: tuple[Literal[1]]
    reveal_type(t[0:2])  # revealed: tuple[Literal[1], Literal["a"]]
    reveal_type(t[0:4])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
    reveal_type(t[0:5])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
    reveal_type(t[1:3])  # revealed: tuple[Literal["a"], None]

    reveal_type(t[-2:4])  # revealed: tuple[None, Literal[b"b"]]
    reveal_type(t[-3:-1])  # revealed: tuple[Literal["a"], None]
    reveal_type(t[-10:10])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]

    reveal_type(t[0:])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
    reveal_type(t[2:])  # revealed: tuple[None, Literal[b"b"]]
    reveal_type(t[4:])  # revealed: tuple[()]
    reveal_type(t[:0])  # revealed: tuple[()]
    reveal_type(t[:2])  # revealed: tuple[Literal[1], Literal["a"]]
    reveal_type(t[:10])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]
    reveal_type(t[:])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]

    reveal_type(t[::-1])  # revealed: tuple[Literal[b"b"], None, Literal["a"], Literal[1]]
    reveal_type(t[::2])  # revealed: tuple[Literal[1], None]
    reveal_type(t[-2:-5:-1])  # revealed: tuple[None, Literal["a"], Literal[1]]
    reveal_type(t[::-2])  # revealed: tuple[Literal[b"b"], Literal["a"]]
    reveal_type(t[-1::-3])  # revealed: tuple[Literal[b"b"], Literal[1]]

    reveal_type(t[None:2:None])  # revealed: tuple[Literal[1], Literal["a"]]
    reveal_type(t[1:None:1])  # revealed: tuple[Literal["a"], None, Literal[b"b"]]
    reveal_type(t[None:None:None])  # revealed: tuple[Literal[1], Literal["a"], None, Literal[b"b"]]

    start = 1
    stop = None
    step = 2
    reveal_type(t[start:stop:step])  # revealed: tuple[Literal["a"], Literal[b"b"]]

    reveal_type(t[False:True])  # revealed: tuple[Literal[1]]
    reveal_type(t[True:3])  # revealed: tuple[Literal["a"], None]

    t[0:4:0]  # error: [zero-stepsize-in-slice]
    t[:4:0]  # error: [zero-stepsize-in-slice]
    t[0::0]  # error: [zero-stepsize-in-slice]
    t[::0]  # error: [zero-stepsize-in-slice]

    tuple_slice = t[m:n]
    reveal_type(tuple_slice)  # revealed: tuple[Literal[1, "a", b"b"] | None, ...]
```

## Slices of homogeneous and mixed tuples

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Literal

def homogeneous(t: tuple[str, ...]) -> None:
    reveal_type(t[0])  # revealed: str
    reveal_type(t[1])  # revealed: str
    reveal_type(t[2])  # revealed: str
    reveal_type(t[3])  # revealed: str

    reveal_type(t[-1])  # revealed: str
    reveal_type(t[-2])  # revealed: str
    reveal_type(t[-3])  # revealed: str
    reveal_type(t[-4])  # revealed: str

def mixed(s: tuple[str, ...]) -> None:
    t = (1, 2, 3) + s + (8, 9, 10)

    reveal_type(t[0])  # revealed: Literal[1]
    reveal_type(t[1])  # revealed: Literal[2]
    reveal_type(t[2])  # revealed: Literal[3]
    reveal_type(t[3])  # revealed: str | Literal[8]
    reveal_type(t[4])  # revealed: str | Literal[8, 9]
    reveal_type(t[5])  # revealed: str | Literal[8, 9, 10]

    reveal_type(t[-1])  # revealed: Literal[10]
    reveal_type(t[-2])  # revealed: Literal[9]
    reveal_type(t[-3])  # revealed: Literal[8]
    reveal_type(t[-4])  # revealed: Literal[3] | str
    reveal_type(t[-5])  # revealed: Literal[2, 3] | str
    reveal_type(t[-6])  # revealed: Literal[1, 2, 3] | str
```

## `tuple` as generic alias

For tuple instances, we can track more detailed information about the length and element types of
the tuple. This information carries over to the generic alias that the tuple is an instance of.

```py
def _(a: tuple, b: tuple[int], c: tuple[int, str], d: tuple[int, ...]) -> None:
    reveal_type(a)  # revealed: tuple[Unknown, ...]
    reveal_type(b)  # revealed: tuple[int]
    reveal_type(c)  # revealed: tuple[int, str]
    reveal_type(d)  # revealed: tuple[int, ...]

reveal_type(tuple)  # revealed: <class 'tuple'>
reveal_type(tuple[int])  # revealed: <class 'tuple[int]'>
reveal_type(tuple[int, str])  # revealed: <class 'tuple[int, str]'>
reveal_type(tuple[int, ...])  # revealed: <class 'tuple[int, ...]'>
```

## Inheritance

```toml
[environment]
python-version = "3.9"
```

```py
class A(tuple[int, str]): ...

# revealed: tuple[<class 'A'>, <class 'tuple[int, str]'>, <class 'Sequence[int | str]'>, <class 'Reversible[int | str]'>, <class 'Collection[int | str]'>, <class 'Iterable[int | str]'>, <class 'Container[int | str]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(A.__mro__)

class C(tuple): ...

# revealed: tuple[<class 'C'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(C.__mro__)
```

## `typing.Tuple`

### Correspondence with `tuple`

`typing.Tuple` can be used interchangeably with `tuple`:

```py
from typing import Any, Tuple

class A: ...

def _(c: Tuple, d: Tuple[int, A], e: Tuple[Any, ...]):
    reveal_type(c)  # revealed: tuple[Unknown, ...]
    reveal_type(d)  # revealed: tuple[int, A]
    reveal_type(e)  # revealed: tuple[Any, ...]
```

### Inheritance

Inheriting from `Tuple` results in a MRO with `builtins.tuple` and `typing.Generic`. `Tuple` itself
is not a class.

```toml
[environment]
python-version = "3.9"
```

```py
from typing import Tuple

class A(Tuple[int, str]): ...

# revealed: tuple[<class 'A'>, <class 'tuple[int, str]'>, <class 'Sequence[int | str]'>, <class 'Reversible[int | str]'>, <class 'Collection[int | str]'>, <class 'Iterable[int | str]'>, <class 'Container[int | str]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(A.__mro__)

class C(Tuple): ...

# revealed: tuple[<class 'C'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(C.__mro__)
```

### Union subscript access

```py
def test(val: tuple[str] | tuple[int]):
    reveal_type(val[0])  # revealed: str | int

def test2(val: tuple[str, None] | list[int | float]):
    reveal_type(val[0])  # revealed: str | int | float
```

### Union subscript access with non-indexable type

```py
def test3(val: tuple[str] | tuple[int] | int):
    # error: [non-subscriptable] "Cannot subscript object of type `int` with no `__getitem__` method"
    reveal_type(val[0])  # revealed: str | int | Unknown
```

### Intersection subscript access

```py
from ty_extensions import Intersection

class Foo: ...
class Bar: ...

def test4(val: Intersection[tuple[Foo], tuple[Bar]]):
    # TODO: should be `Foo & Bar`
    reveal_type(val[0])  # revealed: @Todo(Subscript expressions on intersections)
```
