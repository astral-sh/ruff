# Tuple subscripts

## Indexing

```toml
[environment]
python-version = "3.11"
```

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

Precise types for index operations are also inferred for tuple subclasses:

```py
class I0: ...
class I1: ...
class I2: ...
class I3: ...
class I5: ...
class HeterogeneousSubclass0(tuple[()]): ...

# revealed: Overload[(self, index: SupportsIndex, /) -> Never, (self, index: slice[Any, Any, Any], /) -> tuple[()]]
reveal_type(HeterogeneousSubclass0.__getitem__)

def f0(h0: HeterogeneousSubclass0, i: int):
    # error: [index-out-of-bounds]
    reveal_type(h0[0])  # revealed: Unknown
    # error: [index-out-of-bounds]
    reveal_type(h0[1])  # revealed: Unknown
    # error: [index-out-of-bounds]
    reveal_type(h0[-1])  # revealed: Unknown

    reveal_type(h0[i])  # revealed: Never

class HeterogeneousSubclass1(tuple[I0]): ...

# revealed: Overload[(self, index: SupportsIndex, /) -> I0, (self, index: slice[Any, Any, Any], /) -> tuple[I0, ...]]
reveal_type(HeterogeneousSubclass1.__getitem__)

def f0(h1: HeterogeneousSubclass1, i: int):
    reveal_type(h1[0])  # revealed: I0
    # error: [index-out-of-bounds]
    reveal_type(h1[1])  # revealed: Unknown
    reveal_type(h1[-1])  # revealed: I0
    reveal_type(h1[i])  # revealed: I0

# Element at index 2 is deliberately the same as the element at index 1,
# to illustrate that the `__getitem__` overloads for these two indices are combined
class HeterogeneousSubclass4(tuple[I0, I1, I0, I3]): ...

# revealed: Overload[(self, index: Literal[-4, -2, 0, 2], /) -> I0, (self, index: Literal[-3, 1], /) -> I1, (self, index: Literal[-1, 3], /) -> I3, (self, index: SupportsIndex, /) -> I0 | I1 | I3, (self, index: slice[Any, Any, Any], /) -> tuple[I0 | I1 | I3, ...]]
reveal_type(HeterogeneousSubclass4.__getitem__)

def f(h4: HeterogeneousSubclass4, i: int):
    reveal_type(h4[0])  # revealed: I0
    reveal_type(h4[1])  # revealed: I1
    reveal_type(h4[2])  # revealed: I0
    reveal_type(h4[3])  # revealed: I3
    reveal_type(h4[-1])  # revealed: I3
    reveal_type(h4[-2])  # revealed: I0
    reveal_type(h4[-3])  # revealed: I1
    reveal_type(h4[-4])  # revealed: I0
    reveal_type(h4[i])  # revealed: I0 | I1 | I3

class MixedSubclass(tuple[I0, *tuple[I1, ...], I2, I3, I2, I5]): ...

# revealed: Overload[(self, index: Literal[0], /) -> I0, (self, index: Literal[-5], /) -> I1 | I0, (self, index: Literal[-1], /) -> I5, (self, index: Literal[1], /) -> I1 | I2, (self, index: Literal[-4, -2], /) -> I2, (self, index: Literal[2, 3], /) -> I1 | I2 | I3, (self, index: Literal[-3], /) -> I3, (self, index: Literal[4], /) -> I1 | I2 | I3 | I5, (self, index: SupportsIndex, /) -> I0 | I1 | I2 | I3 | I5, (self, index: slice[Any, Any, Any], /) -> tuple[I0 | I1 | I2 | I3 | I5, ...]]
reveal_type(MixedSubclass.__getitem__)

def g(m: MixedSubclass, i: int):
    reveal_type(m[0])  # revealed: I0
    reveal_type(m[1])  # revealed: I1 | I2
    reveal_type(m[2])  # revealed: I1 | I2 | I3
    reveal_type(m[3])  # revealed: I1 | I2 | I3
    reveal_type(m[4])  # revealed: I1 | I2 | I3 | I5
    reveal_type(m[5])  # revealed: I1 | I2 | I3 | I5
    reveal_type(m[10])  # revealed: I1 | I2 | I3 | I5

    reveal_type(m[-1])  # revealed: I5
    reveal_type(m[-2])  # revealed: I2
    reveal_type(m[-3])  # revealed: I3
    reveal_type(m[-4])  # revealed: I2
    reveal_type(m[-5])  # revealed: I0 | I1
    reveal_type(m[-6])  # revealed: I0 | I1
    reveal_type(m[-10])  # revealed: I0 | I1

    reveal_type(m[i])  # revealed: I0 | I1 | I2 | I3 | I5

class MixedSubclass2(tuple[I0, I1, *tuple[I2, ...], I3]): ...

# revealed: Overload[(self, index: Literal[0], /) -> I0, (self, index: Literal[-2], /) -> I2 | I1, (self, index: Literal[1], /) -> I1, (self, index: Literal[-3], /) -> I2 | I1 | I0, (self, index: Literal[-1], /) -> I3, (self, index: Literal[2], /) -> I2 | I3, (self, index: SupportsIndex, /) -> I0 | I1 | I2 | I3, (self, index: slice[Any, Any, Any], /) -> tuple[I0 | I1 | I2 | I3, ...]]
reveal_type(MixedSubclass2.__getitem__)

def g(m: MixedSubclass2, i: int):
    reveal_type(m[0])  # revealed: I0
    reveal_type(m[1])  # revealed: I1
    reveal_type(m[2])  # revealed: I2 | I3
    reveal_type(m[3])  # revealed: I2 | I3

    reveal_type(m[-1])  # revealed: I3
    reveal_type(m[-2])  # revealed: I1 | I2
    reveal_type(m[-3])  # revealed: I0 | I1 | I2
    reveal_type(m[-4])  # revealed: I0 | I1 | I2
```

The stdlib API `os.stat` is a commonly used API that returns an instance of a tuple subclass
(`os.stat_result`), and therefore provides a good integration test for tuple subclasses.

```py
import os
import stat
from ty_extensions import reveal_mro

reveal_type(os.stat("my_file.txt"))  # revealed: stat_result
reveal_type(os.stat("my_file.txt")[stat.ST_MODE])  # revealed: int
reveal_type(os.stat("my_file.txt")[stat.ST_ATIME])  # revealed: int | float

# revealed: (<class 'stat_result'>, <class 'structseq[int | float]'>, <class 'tuple[int, int, int, int, int, int, int, int | float, int | float, int | float]'>, <class 'Sequence[int | float]'>, <class 'Reversible[int | float]'>, <class 'Collection[int | float]'>, <class 'Iterable[int | float]'>, <class 'Container[int | float]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(os.stat_result)

# There are no specific overloads for the `float` elements in `os.stat_result`,
# because the fallback `(self, index: SupportsIndex, /) -> int | float` overload
# gives the right result for those elements in the tuple, and we aim to synthesize
# the minimum number of overloads for any given tuple
#
# revealed: Overload[(self, index: Literal[-10, -9, -8, -7, -6, -5, -4, 0, 1, 2, 3, 4, 5, 6], /) -> int, (self, index: SupportsIndex, /) -> int | float, (self, index: slice[Any, Any, Any], /) -> tuple[int | float, ...]]
reveal_type(os.stat_result.__getitem__)
```

But perhaps the most commonly used tuple subclass instance is the singleton `sys.version_info`:

```py
import sys

# revealed: Overload[(self, index: Literal[-5, 0], /) -> Literal[3], (self, index: Literal[-4, 1], /) -> Literal[11], (self, index: Literal[-3, -1, 2, 4], /) -> int, (self, index: Literal[-2, 3], /) -> Literal["alpha", "beta", "candidate", "final"], (self, index: SupportsIndex, /) -> int | Literal["alpha", "beta", "candidate", "final"], (self, index: slice[Any, Any, Any], /) -> tuple[int | Literal["alpha", "beta", "candidate", "final"], ...]]
reveal_type(type(sys.version_info).__getitem__)
```

Because of the synthesized `__getitem__` overloads we synthesize for tuples and tuple subclasses,
tuples are naturally understood as being subtypes of protocols that have precise return types from
`__getitem__` method members:

```py
from typing import Protocol, Literal
from ty_extensions import static_assert, is_subtype_of

class IntFromZeroSubscript(Protocol):
    def __getitem__(self, index: Literal[0], /) -> int: ...

static_assert(is_subtype_of(tuple[int, str], IntFromZeroSubscript))

class TupleSubclass(tuple[int, str]): ...

static_assert(is_subtype_of(TupleSubclass, IntFromZeroSubscript))
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

class I0: ...
class I1: ...
class I2: ...
class I3: ...
class HeterogeneousTupleSubclass(tuple[I0, I1, I2, I3]): ...

def __(t: HeterogeneousTupleSubclass, m: int, n: int):
    reveal_type(t[0:0])  # revealed: tuple[()]
    reveal_type(t[0:1])  # revealed: tuple[I0]
    reveal_type(t[0:2])  # revealed: tuple[I0, I1]
    reveal_type(t[0:4])  # revealed: tuple[I0, I1, I2, I3]
    reveal_type(t[0:5])  # revealed: tuple[I0, I1, I2, I3]
    reveal_type(t[1:3])  # revealed: tuple[I1, I2]

    reveal_type(t[-2:4])  # revealed: tuple[I2, I3]
    reveal_type(t[-3:-1])  # revealed: tuple[I1, I2]
    reveal_type(t[-10:10])  # revealed: tuple[I0, I1, I2, I3]

    reveal_type(t[0:])  # revealed: tuple[I0, I1, I2, I3]
    reveal_type(t[2:])  # revealed: tuple[I2, I3]
    reveal_type(t[4:])  # revealed: tuple[()]
    reveal_type(t[:0])  # revealed: tuple[()]
    reveal_type(t[:2])  # revealed: tuple[I0, I1]
    reveal_type(t[:10])  # revealed: tuple[I0, I1, I2, I3]
    reveal_type(t[:])  # revealed: tuple[I0, I1, I2, I3]

    reveal_type(t[::-1])  # revealed: tuple[I3, I2, I1, I0]
    reveal_type(t[::2])  # revealed: tuple[I0, I2]
    reveal_type(t[-2:-5:-1])  # revealed: tuple[I2, I1, I0]
    reveal_type(t[::-2])  # revealed: tuple[I3, I1]
    reveal_type(t[-1::-3])  # revealed: tuple[I3, I0]

    reveal_type(t[None:2:None])  # revealed: tuple[I0, I1]
    reveal_type(t[1:None:1])  # revealed: tuple[I1, I2, I3]
    reveal_type(t[None:None:None])  # revealed: tuple[I0, I1, I2, I3]

    start = 1
    stop = None
    step = 2
    reveal_type(t[start:stop:step])  # revealed: tuple[I1, I3]

    reveal_type(t[False:True])  # revealed: tuple[I0]
    reveal_type(t[True:3])  # revealed: tuple[I1, I2]

    t[0:4:0]  # error: [zero-stepsize-in-slice]
    t[:4:0]  # error: [zero-stepsize-in-slice]
    t[0::0]  # error: [zero-stepsize-in-slice]
    t[::0]  # error: [zero-stepsize-in-slice]

    tuple_slice = t[m:n]
    reveal_type(tuple_slice)  # revealed: tuple[I0 | I1 | I2 | I3, ...]
```

## Indexes into homogeneous and mixed tuples

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

def mixed(t: tuple[Literal[1], Literal[2], Literal[3], *tuple[str, ...], Literal[8], Literal[9], Literal[10]]) -> None:
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

```py
from typing import Any

def _(a: type[tuple], b: type[tuple[int]], c: type[tuple[int, ...]], d: type[tuple[Any, ...]]) -> None:
    reveal_type(a)  # revealed: type[tuple[Unknown, ...]]
    reveal_type(b)  # revealed: type[tuple[int]]
    reveal_type(c)  # revealed: type[tuple[int, ...]]
    reveal_type(d)  # revealed: type[tuple[Any, ...]]
```

## Inheritance

```toml
[environment]
python-version = "3.9"
```

```py
from ty_extensions import reveal_mro

class A(tuple[int, str]): ...

# revealed: (<class 'A'>, <class 'tuple[int, str]'>, <class 'Sequence[int | str]'>, <class 'Reversible[int | str]'>, <class 'Collection[int | str]'>, <class 'Iterable[int | str]'>, <class 'Container[int | str]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(A)

class C(tuple): ...

# revealed: (<class 'C'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(C)
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
from ty_extensions import reveal_mro

class A(Tuple[int, str]): ...

# revealed: (<class 'A'>, <class 'tuple[int, str]'>, <class 'Sequence[int | str]'>, <class 'Reversible[int | str]'>, <class 'Collection[int | str]'>, <class 'Iterable[int | str]'>, <class 'Container[int | str]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(A)

class C(Tuple): ...

# revealed: (<class 'C'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(C)
```

## Union subscript access

```py
def test(val: tuple[str] | tuple[int]):
    reveal_type(val[0])  # revealed: str | int

def test2(val: tuple[str, None] | list[int | float]):
    reveal_type(val[0])  # revealed: str | int | float
```

## Union subscript access with non-indexable type

```py
def test3(val: tuple[str] | tuple[int] | int):
    # error: [not-subscriptable] "Cannot subscript object of type `int` with no `__getitem__` method"
    reveal_type(val[0])  # revealed: str | int | Unknown
```

## Intersection subscript access

```py
from ty_extensions import Intersection

class Foo: ...
class Bar: ...

def test4(val: Intersection[tuple[Foo], tuple[Bar]]):
    # TODO: should be `Foo & Bar`
    reveal_type(val[0])  # revealed: @Todo(Subscript expressions on intersections)
```
