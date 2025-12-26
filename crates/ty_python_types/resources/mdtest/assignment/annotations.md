# Assignment with annotations

## Annotation only transparent to local inference

```py
x = 1
x: int
y = x

reveal_type(y)  # revealed: Literal[1]
```

## Violates own annotation

```py
x: int = "foo"  # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
```

## Violates previous annotation

```py
x: int
x = "foo"  # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
```

## Tuple annotations are understood

```toml
[environment]
python-version = "3.12"
```

`module.py`:

```py
from typing_extensions import Unpack

a: tuple[()] = ()
b: tuple[int] = (42,)
c: tuple[str, int] = ("42", 42)
d: tuple[tuple[str, str], tuple[int, int]] = (("foo", "foo"), (42, 42))
e: tuple[str, ...] = ()
f: tuple[str, *tuple[int, ...], bytes] = ("42", b"42")
g: tuple[str, Unpack[tuple[int, ...]], bytes] = ("42", b"42")
h: tuple[list[int], list[int]] = ([], [])
i: tuple[str | int, str | int] = (42, 42)
j: tuple[str | int] = (42,)
```

`script.py`:

```py
from module import a, b, c, d, e, f, g, h, i, j

reveal_type(a)  # revealed: tuple[()]
reveal_type(b)  # revealed: tuple[int]
reveal_type(c)  # revealed: tuple[str, int]
reveal_type(d)  # revealed: tuple[tuple[str, str], tuple[int, int]]
reveal_type(e)  # revealed: tuple[str, ...]

reveal_type(f)  # revealed: tuple[str, *tuple[int, ...], bytes]
reveal_type(g)  # revealed: tuple[@Todo(PEP 646), ...]

reveal_type(h)  # revealed: tuple[list[int], list[int]]
reveal_type(i)  # revealed: tuple[str | int, str | int]
reveal_type(j)  # revealed: tuple[str | int]
```

## Incorrect tuple assignments are complained about

```py
# error: [invalid-assignment] "Object of type `tuple[Literal[1], Literal[2]]` is not assignable to `tuple[()]`"
a: tuple[()] = (1, 2)

# error: [invalid-assignment] "Object of type `tuple[Literal["foo"]]` is not assignable to `tuple[int]`"
b: tuple[int] = ("foo",)

# error: [invalid-assignment]
c: tuple[str | int, str] = ([], "foo")
```

## Collection literal annotations are understood

```toml
[environment]
python-version = "3.12"
```

```py
import typing

a: list[int] = [1, 2, 3]
reveal_type(a)  # revealed: list[int]

b: list[int | str] = [1, 2, 3]
reveal_type(b)  # revealed: list[int | str]

c: typing.List[int] = [1, 2, 3]
reveal_type(c)  # revealed: list[int]

d: list[typing.Any] = []
reveal_type(d)  # revealed: list[Any]

e: set[int] = {1, 2, 3}
reveal_type(e)  # revealed: set[int]

f: set[int | str] = {1, 2, 3}
reveal_type(f)  # revealed: set[int | str]

g: typing.Set[int] = {1, 2, 3}
reveal_type(g)  # revealed: set[int]

h: list[list[int]] = [[], [42]]
reveal_type(h)  # revealed: list[list[int]]

i: list[typing.Any] = [1, 2, "3", ([4],)]
reveal_type(i)  # revealed: list[Any]

j: list[tuple[str | int, ...]] = [(1, 2), ("foo", "bar"), ()]
reveal_type(j)  # revealed: list[tuple[str | int, ...]]

k: list[tuple[list[int], ...]] = [([],), ([1, 2], [3, 4]), ([5], [6], [7])]
reveal_type(k)  # revealed: list[tuple[list[int], ...]]

l: tuple[list[int], *tuple[list[typing.Any], ...], list[str]] = ([1, 2, 3], [4, 5, 6], [7, 8, 9], ["10", "11", "12"])
reveal_type(l)  # revealed: tuple[list[int], list[Any], list[Any], list[str]]

type IntList = list[int]

m: IntList = [1, 2, 3]
reveal_type(m)  # revealed: list[int]

n: list[typing.Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(n)  # revealed: list[Literal[1, 2, 3]]

o: list[typing.LiteralString] = ["a", "b", "c"]
reveal_type(o)  # revealed: list[LiteralString]

p: dict[int, int] = {}
reveal_type(p)  # revealed: dict[int, int]

q: dict[int | str, int] = {1: 1, 2: 2, 3: 3}
reveal_type(q)  # revealed: dict[int | str, int]

r: dict[int | str, int | str] = {1: 1, 2: 2, 3: 3}
reveal_type(r)  # revealed: dict[int | str, int | str]

s: dict[int | str, int | str]
s = {1: 1, 2: 2, 3: 3}
reveal_type(s)  # revealed: dict[int | str, int | str]
(s := {1: 1, 2: 2, 3: 3})
reveal_type(s)  # revealed: dict[int | str, int | str]
```

## Optional collection literal annotations are understood

```toml
[environment]
python-version = "3.12"
```

```py
import typing

a: list[int] | None = [1, 2, 3]
reveal_type(a)  # revealed: list[int]

b: list[int | str] | None = [1, 2, 3]
reveal_type(b)  # revealed: list[int | str]

c: typing.List[int] | None = [1, 2, 3]
reveal_type(c)  # revealed: list[int]

d: list[typing.Any] | None = []
reveal_type(d)  # revealed: list[Any]

e: set[int] | None = {1, 2, 3}
reveal_type(e)  # revealed: set[int]

f: set[int | str] | None = {1, 2, 3}
reveal_type(f)  # revealed: set[int | str]

g: typing.Set[int] | None = {1, 2, 3}
reveal_type(g)  # revealed: set[int]

h: list[list[int]] | None = [[], [42]]
reveal_type(h)  # revealed: list[list[int]]

i: list[typing.Any] | None = [1, 2, "3", ([4],)]
reveal_type(i)  # revealed: list[Any]

j: list[tuple[str | int, ...]] | None = [(1, 2), ("foo", "bar"), ()]
reveal_type(j)  # revealed: list[tuple[str | int, ...]]

k: list[tuple[list[int], ...]] | None = [([],), ([1, 2], [3, 4]), ([5], [6], [7])]
reveal_type(k)  # revealed: list[tuple[list[int], ...]]

l: tuple[list[int], *tuple[list[typing.Any], ...], list[str]] | None = ([1, 2, 3], [4, 5, 6], [7, 8, 9], ["10", "11", "12"])
reveal_type(l)  # revealed: tuple[list[int], list[Any], list[Any], list[str]]

type IntList = list[int]

m: IntList | None = [1, 2, 3]
reveal_type(m)  # revealed: list[int]

n: list[typing.Literal[1, 2, 3]] | None = [1, 2, 3]
reveal_type(n)  # revealed: list[Literal[1, 2, 3]]

o: list[typing.LiteralString] | None = ["a", "b", "c"]
reveal_type(o)  # revealed: list[LiteralString]

p: dict[int, int] | None = {}
reveal_type(p)  # revealed: dict[int, int]

q: dict[int | str, int] | None = {1: 1, 2: 2, 3: 3}
reveal_type(q)  # revealed: dict[int | str, int]

r: dict[int | str, int | str] | None = {1: 1, 2: 2, 3: 3}
reveal_type(r)  # revealed: dict[int | str, int | str]
```

## Incorrect collection literal assignments are complained about

```py
# error: [invalid-assignment] "Object of type `list[str | int]` is not assignable to `list[str]`"
a: list[str] = [1, 2, 3]

# error: [invalid-assignment] "Object of type `set[int | str]` is not assignable to `set[int]`"
b: set[int] = {1, 2, "3"}
```

## Generic constructor annotations are understood

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any

class X[T]:
    def __init__(self, value: T):
        self.value = value

x1: X[int] = X(1)
reveal_type(x1)  # revealed: X[int]

x2: X[int | None] = X(1)
reveal_type(x2)  # revealed: X[int | None]

x3: X[int | None] | None = X(1)
reveal_type(x3)  # revealed: X[int | None]

def _[T](x1: X[T]):
    x2: X[T | int] = X(x1.value)
    reveal_type(x2)  # revealed: X[T@_ | int]

x4: X[Any] = X(1)
reveal_type(x4)  # revealed: X[Any]

def _(flag: bool):
    x5: X[int | None] = X(1) if flag else X(2)
    reveal_type(x5)  # revealed: X[int | None]
```

```py
from dataclasses import dataclass

@dataclass
class Y[T]:
    value: T

y1 = Y(value=1)
reveal_type(y1)  # revealed: Y[int]

y2: Y[Any] = Y(value=1)
reveal_type(y2)  # revealed: Y[Any]
```

```py
class Z[T]:
    value: T

    def __new__(cls, value: T):
        return super().__new__(cls)

z1 = Z(1)
reveal_type(z1)  # revealed: Z[int]

z2: Z[Any] = Z(1)
reveal_type(z2)  # revealed: Z[Any]
```

## PEP-604 annotations are supported

```py
def foo(v: str | int | None, w: str | str | None, x: str | str):
    reveal_type(v)  # revealed: str | int | None
    reveal_type(w)  # revealed: str | None
    reveal_type(x)  # revealed: str
```

## PEP-604 in non-type-expression context

### In Python 3.10 and later

```toml
[environment]
python-version = "3.10"
```

```py
IntOrStr = int | str
```

### Earlier versions

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.9"
```

```py
# error: [unsupported-operator]
IntOrStr = int | str
```

## Attribute expressions in type annotations are understood

```py
import builtins

int = "foo"
a: builtins.int = 42

# error: [invalid-assignment] "Object of type `Literal["bar"]` is not assignable to `int`"
b: builtins.int = "bar"

c: builtins.tuple[builtins.tuple[builtins.int, builtins.int], builtins.int] = ((42, 42), 42)

# error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `tuple[tuple[int, int], int]`"
c: builtins.tuple[builtins.tuple[builtins.int, builtins.int], builtins.int] = "foo"
```

## Future annotations are deferred

```py
from __future__ import annotations

x: Foo

class Foo: ...

x = Foo()
reveal_type(x)  # revealed: Foo
```

## Annotations in stub files are deferred

```pyi
x: Foo

class Foo: ...

x = Foo()
reveal_type(x)  # revealed: Foo
```

## Annotations are deferred by default in Python 3.14 and later

```toml
[environment]
python-version = "3.14"
```

```py
x: Foo

class Foo: ...

x = Foo()
reveal_type(x)  # revealed: Foo
```

## Annotated assignments in stub files are inferred correctly

```pyi
x: int = 1
reveal_type(x) # revealed: Literal[1]
```

## Annotations influence generic call inference

```toml
[environment]
python-version = "3.12"
```

`generic_list.py`:

```py
from typing import Literal

def f[T](x: T) -> list[T]:
    return [x]

a = f("a")
reveal_type(a)  # revealed: list[str]

b: list[int | Literal["a"]] = f("a")
reveal_type(b)  # revealed: list[int | Literal["a"]]

c: list[int | str] = f("a")
reveal_type(c)  # revealed: list[int | str]

d: list[int | tuple[int, int]] = f((1, 2))
reveal_type(d)  # revealed: list[int | tuple[int, int]]

e: list[int] = f(True)
reveal_type(e)  # revealed: list[int]

# error: [invalid-assignment] "Object of type `list[int | str]` is not assignable to `list[int]`"
g: list[int] = f("a")

# error: [invalid-assignment] "Object of type `list[str]` is not assignable to `tuple[int]`"
h: tuple[int] = f("a")

def f2[T: int](x: T) -> T:
    return x

i: int = f2(True)
reveal_type(i)  # revealed: Literal[True]

j: int | str = f2(True)
reveal_type(j)  # revealed: Literal[True]
```

A function's arguments are also inferred using the type context:

`typed_dict.py`:

```py
from typing import TypedDict

class TD(TypedDict):
    x: int

def f[T](x: list[T]) -> T:
    return x[0]

a: TD = f([{"x": 0}, {"x": 1}])
reveal_type(a)  # revealed: TD

b: TD | None = f([{"x": 0}, {"x": 1}])
reveal_type(b)  # revealed: TD

# error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
# error: [invalid-key] "Unknown key "y" for TypedDict `TD`"
# error: [invalid-assignment] "Object of type `TD | dict[Unknown | str, Unknown | int]` is not assignable to `TD`"
c: TD = f([{"y": 0}, {"x": 1}])

# error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
# error: [invalid-key] "Unknown key "y" for TypedDict `TD`"
# error: [invalid-assignment] "Object of type `TD | None | dict[Unknown | str, Unknown | int]` is not assignable to `TD | None`"
c: TD | None = f([{"y": 0}, {"x": 1}])
```

But not in a way that leads to assignability errors:

`dict_any.py`:

```py
from typing import TypedDict, Any

class TD(TypedDict, total=False):
    x: str

class TD2(TypedDict):
    x: str

def f(self, dt: dict[str, Any], key: str):
    x1: TD = dt.get(key, {})
    reveal_type(x1)  # revealed: Any

    x2: TD = dt.get(key, {"x": 0})
    reveal_type(x2)  # revealed: Any

    x3: TD | None = dt.get(key, {})
    reveal_type(x3)  # revealed: Any

    x4: TD | None = dt.get(key, {"x": 0})
    reveal_type(x4)  # revealed: Any

    x5: TD2 = dt.get(key, {})
    reveal_type(x5)  # revealed: Any

    x6: TD2 = dt.get(key, {"x": 0})
    reveal_type(x6)  # revealed: Any

    x7: TD2 | None = dt.get(key, {})
    reveal_type(x7)  # revealed: Any

    x8: TD2 | None = dt.get(key, {"x": 0})
    reveal_type(x8)  # revealed: Any
```

## Prefer the declared type of generic classes

```toml
[environment]
python-version = "3.14"
```

```py
from typing import Any

def f[T](x: T) -> list[T]:
    return [x]

def f2[T](x: T) -> list[T] | None:
    return [x]

def f3[T](x: T) -> list[T] | dict[T, T]:
    return [x]

a = f(1)
reveal_type(a)  # revealed: list[int]

b: list[Any] = f(1)
reveal_type(b)  # revealed: list[Any]

c: list[Any] = [1]
reveal_type(c)  # revealed: list[Any]

d: list[Any] | None = f(1)
reveal_type(d)  # revealed: list[Any]

e: list[Any] | None = [1]
reveal_type(e)  # revealed: list[Any]

f: list[Any] | None = f2(1)
# TODO: Better constraint solver.
reveal_type(f)  # revealed: list[int] | None

g: list[Any] | dict[Any, Any] = f3(1)
# TODO: Better constraint solver.
reveal_type(g)  # revealed: list[int] | dict[int, int]
```

We only prefer the declared type if it is in non-covariant position.

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

def bivariant[T](x: T) -> Bivariant[T]:
    return Bivariant()

def covariant[T](x: T) -> Covariant[T]:
    return Covariant()

def contravariant[T](x: T) -> Contravariant[T]:
    return Contravariant()

def invariant[T](x: T) -> Invariant[T]:
    return Invariant()

x1 = bivariant(1)
x2 = covariant(1)
x3 = contravariant(1)
x4 = invariant(1)

reveal_type(x1)  # revealed: Bivariant[Literal[1]]
reveal_type(x2)  # revealed: Covariant[Literal[1]]
reveal_type(x3)  # revealed: Contravariant[int]
reveal_type(x4)  # revealed: Invariant[int]

x5: Bivariant[Any] = bivariant(1)
x6: Covariant[Any] = covariant(1)
x7: Contravariant[Any] = contravariant(1)
x8: Invariant[Any] = invariant(1)

reveal_type(x5)  # revealed: Bivariant[Literal[1]]
reveal_type(x6)  # revealed: Covariant[Literal[1]]
reveal_type(x7)  # revealed: Contravariant[Any]
reveal_type(x8)  # revealed: Invariant[Any]
```

```py
class X[T]:
    def __init__(self: X[None]): ...
    def pop(self) -> T:
        raise NotImplementedError

x1: X[int | None] = X()
reveal_type(x1)  # revealed: X[None]
```

## Narrow generic unions

```toml
[environment]
python-version = "3.12"
```

```py
from typing import reveal_type, TypedDict

def identity[T](x: T) -> T:
    return x

def _(narrow: dict[str, str], target: list[str] | dict[str, str] | None):
    target = identity(narrow)
    reveal_type(target)  # revealed: dict[str, str]

def _(narrow: list[str], target: list[str] | dict[str, str] | None):
    target = identity(narrow)
    reveal_type(target)  # revealed: list[str]

def _(narrow: list[str] | dict[str, str], target: list[str] | dict[str, str] | None):
    target = identity(narrow)
    reveal_type(target)  # revealed: list[str] | dict[str, str]

class TD(TypedDict):
    x: int

def _(target: list[TD] | dict[str, TD] | None):
    target = identity([{"x": 1}])
    reveal_type(target)  # revealed: list[TD]

def _(target: list[TD] | dict[str, TD] | None):
    target = identity({"x": {"x": 1}})
    reveal_type(target)  # revealed: dict[str, TD]
```

## Prefer the inferred type of non-generic classes

```toml
[environment]
python-version = "3.12"
```

```py
def identity[T](x: T) -> T:
    return x

def lst[T](x: T) -> list[T]:
    return [x]

def _(i: int):
    a: int | None = i
    b: int | None = identity(i)
    c: int | str | None = identity(i)
    reveal_type(a)  # revealed: int
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: int

    a: list[int | None] | None = [i]
    b: list[int | None] | None = identity([i])
    c: list[int | None] | int | None = identity([i])
    reveal_type(a)  # revealed: list[int | None]
    reveal_type(b)  # revealed: list[int | None]
    reveal_type(c)  # revealed: list[int | None]

    a: list[int | None] | None = [i]
    b: list[int | None] | None = lst(i)
    c: list[int | None] | int | None = lst(i)
    reveal_type(a)  # revealed: list[int | None]
    reveal_type(b)  # revealed: list[int | None]
    reveal_type(c)  # revealed: list[int | None]

    a: list | None = []
    b: list | None = identity([])
    c: list | int | None = identity([])
    reveal_type(a)  # revealed: list[Unknown]
    reveal_type(b)  # revealed: list[Unknown]
    reveal_type(c)  # revealed: list[Unknown]

def f[T](x: list[T]) -> T:
    return x[0]

def _(a: int, b: str, c: int | str):
    x1: int = f(lst(a))
    reveal_type(x1)  # revealed: int

    x2: int | str = f(lst(a))
    reveal_type(x2)  # revealed: int

    x3: int | None = f(lst(a))
    reveal_type(x3)  # revealed: int

    x4: str = f(lst(b))
    reveal_type(x4)  # revealed: str

    x5: int | str = f(lst(b))
    reveal_type(x5)  # revealed: str

    x6: str | None = f(lst(b))
    reveal_type(x6)  # revealed: str

    x7: int | str = f(lst(c))
    reveal_type(x7)  # revealed: int | str

    x8: int | str = f(lst(c))
    reveal_type(x8)  # revealed: int | str

    # TODO: Ideally this would reveal `int | str`. This is a known limitation of our
    # call inference solver, and would # require an extra inference attempt without type
    # context, or with type context # of subsets of the union, both of which are impractical
    # for performance reasons.
    x9: int | str | None = f(lst(c))
    reveal_type(x9)  # revealed: int | str | None
```

## Forward annotation with unclosed string literal

Regression test for [#1611](https://github.com/astral-sh/ty/issues/1611).

<!-- blacken-docs:off -->

```py
# error: [invalid-syntax]
# error: [invalid-syntax-in-forward-annotation]
a:'
```

<!-- blacken-docs:on -->
