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

## Numbers special case

```py
from numbers import Number

# snapshot: invalid-assignment
a: Number = 1
```

```snapshot
error[invalid-assignment]: Object of type `Literal[1]` is not assignable to `Number`
 --> src/mdtest_snippet.py:4:4
  |
4 | a: Number = 1
  |    ------   ^ Incompatible value of type `Literal[1]`
  |    |
  |    Declared type
  |
info: Types from the `numbers` module aren't supported for static type checking
help: Consider using a protocol instead, such as `typing.SupportsFloat`
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
reveal_type(g)  # revealed: tuple[str, *tuple[int, ...], bytes]

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

## Mutually assignable annotated assignments use the declared type

When an annotated assignment has a value whose inferred type is assignable to the declared type, the
binding uses the declared type if the declared type is also assignable back to the inferred type.
This indicates that we are dealing with difference in precision (graduality) rather than a narrowed
static type; in that case we want to prefer the user's annotation.

The actual inferred type of the right-hand side is still used to validate the assignment.

```py
from typing import Any

def returns_list_any() -> list[Any]:
    return [1]

def returns_list_int() -> list[int]:
    return [1]

def returns_any() -> Any:
    return 1

v1: Any = 1
reveal_type(v1)  # revealed: Any

v2: int = returns_any()
reveal_type(v2)  # revealed: int

v3: list[Any] = returns_list_int()
reveal_type(v3)  # revealed: list[Any]

v4: list[int] = returns_list_any()
reveal_type(v4)  # revealed: list[int]

v4: object = returns_list_int()
reveal_type(v4)  # revealed: list[int]

# error: [invalid-assignment] "Object of type `list[int]` is not assignable to `list[str]`"
invalid: list[str] = returns_list_int()
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

```toml
[environment]
python-version = "3.9"
```

```py
# snapshot: unsupported-operator
IntOrStr = int | str
```

```snapshot
error[unsupported-operator]: Unsupported `|` operation
 --> src/mdtest_snippet.py:2:12
  |
2 | IntOrStr = int | str
  |            ---^^^---
  |            |     |
  |            |     Has type `<class 'str'>`
  |            Has type `<class 'int'>`
  |
info: PEP 604 `|` unions are only available on Python 3.10+ unless they are quoted
info: Python 3.9 was assumed when resolving types because it was specified on the command line
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
reveal_type(x)  # revealed: Literal[1]
```

## Annotations influence generic call inference

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal, Sequence

def f[T](x: T) -> list[T]:
    return [x]

x1 = f("a")
reveal_type(x1)  # revealed: list[str]

x2: list[int | Literal["a"]] = f("a")
reveal_type(x2)  # revealed: list[int | Literal["a"]]

x3: list[int | str] = f("a")
reveal_type(x3)  # revealed: list[int | str]

x4: list[int | tuple[int, int]] = f((1, 2))
reveal_type(x4)  # revealed: list[int | tuple[int, int]]

x5: list[int] = f(True)
reveal_type(x5)  # revealed: list[int]

# error: [invalid-assignment] "Object of type `list[str]` is not assignable to `list[int]`"
x6: list[int] = f("a")

# error: [invalid-assignment] "Object of type `list[str]` is not assignable to `tuple[int]`"
x7: tuple[int] = f("a")

def f2[T: int](x: T) -> T:
    return x

x8: int = f2(True)
reveal_type(x8)  # revealed: Literal[True]

x9: int | str = f2(True)
reveal_type(x9)  # revealed: Literal[True]

x10: list[int | str] | list[int | None] = [1, 2, 3]
reveal_type(x10)  # revealed: list[int | str]

x11: Sequence[int | str] | Sequence[int | None] = [1, 2, 3]
reveal_type(x11)  # revealed: list[int]

x12: list[int] | list[int | None] | list[str | None] = ["1", "2"]
reveal_type(x12)  # revealed: list[str | None]

x13: dict[str, list[int | None]] | dict[str, list[str | None]] = {"a": ["b"]}
reveal_type(x13)  # revealed: dict[str, list[str | None]]

x14 = [{"a": [1], "b": 1}, {"a": [1]}]
x14.append(reveal_type({"b": 1}))  # revealed: dict[str, list[int] | int]
reveal_type(x14)  # revealed: list[dict[str, list[int] | int] | dict[str, list[int]]]
```

## Annotations influence generic call argument inference

```toml
[environment]
python-version = "3.13"
```

A function's arguments are also inferred using the type context:

```py
from typing import TypedDict

class TD(TypedDict):
    x: int

def first[T](x: list[T]) -> T:
    return x[0]

x1: TD = first([{"x": 0}, {"x": 1}])
reveal_type(x1)  # revealed: TD

x2: TD | None = first([{"x": 0}, {"x": 1}])
reveal_type(x2)  # revealed: TD

# error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
# error: [invalid-key] "Unknown key "y" for TypedDict `TD`"
# error: [invalid-assignment] "Object of type `TD | dict[str, int]` is not assignable to `TD`"
x3: TD = first([{"y": 0}, {"x": 1}])

# error: [missing-typed-dict-key] "Missing required key 'x' in TypedDict `TD` constructor"
# error: [invalid-key] "Unknown key "y" for TypedDict `TD`"
# error: [invalid-assignment] "Object of type `TD | None | dict[str, int]` is not assignable to `TD | None`"
x4: TD | None = first([{"y": 0}, {"x": 1}])
```

But not in a way that leads to assignability errors:

```py
from typing import TypedDict, Any

class TD2(TypedDict):
    x: str

def _(dt: dict[str, Any], key: str):
    x1: TD = dt.get(key, {})
    reveal_type(x1)  # revealed: TD

    x2: TD = dt.get(key, {"x": 0})
    reveal_type(x2)  # revealed: TD

    x3: TD | None = dt.get(key, {})
    reveal_type(x3)  # revealed: TD | None

    x4: TD | None = dt.get(key, {"x": 0})
    reveal_type(x4)  # revealed: TD | None

    x5: TD2 = dt.get(key, {})
    reveal_type(x5)  # revealed: TD2

    x6: TD2 = dt.get(key, {"x": 0})
    reveal_type(x6)  # revealed: TD2

    x7: TD2 | None = dt.get(key, {})
    reveal_type(x7)  # revealed: TD2 | None

    x8: TD2 | None = dt.get(key, {"x": 0})
    reveal_type(x8)  # revealed: TD2 | None
```

Partially specialized type context is not ignored:

```py
from typing import TypeVar

U = TypeVar("U", default=Any)

class X: ...

def lst[T](x: T) -> list[T]:
    return [x]

def two_lists[T](x: list[T | int], y: list[T | str]) -> T:
    raise NotImplementedError

def two_lists_default(x: list[U | int], y: list[U | str]) -> U:
    raise NotImplementedError

def dct[K, V](k: K, v: V) -> dict[K, V]:
    return {k: v}

def two_dicts[T](x: dict[T | int, Any], y: dict[T | str, Any]) -> T:
    raise NotImplementedError

def two_dicts_default(x: dict[U | int, Any], y: dict[U | str, Any]) -> U:
    raise NotImplementedError

def _():
    # revealed: list[int | X]
    # revealed: list[str | X]
    x1 = two_lists(reveal_type(lst(X())), reveal_type(lst(X())))
    reveal_type(x1)  # revealed: X

    # revealed: list[int | X]
    # revealed: list[str | X]
    x2 = two_lists(reveal_type([X()]), reveal_type([X()]))
    reveal_type(x2)  # revealed: X

    # revealed: list[int | X]
    # revealed: list[str | X]
    x3 = two_lists_default(reveal_type(lst(X())), reveal_type(lst(X())))
    reveal_type(x3)  # revealed: X

    # revealed: list[int | X]
    # revealed: list[str | X]
    x4 = two_lists_default(reveal_type([X()]), reveal_type([X()]))
    reveal_type(x4)  # revealed: X

    # revealed: dict[int | X, Any]
    # revealed: dict[str | X, Any]
    x5 = two_dicts(reveal_type(dct(X(), X())), reveal_type(dct(X(), X())))
    reveal_type(x5)  # revealed: X

    # revealed: dict[int | X, Any]
    # revealed: dict[str | X, Any]
    x6 = two_dicts(reveal_type({X(): X()}), reveal_type({X(): X()}))
    reveal_type(x6)  # revealed: X

    # revealed: dict[int | X, Any]
    # revealed: dict[str | X, Any]
    x7 = two_dicts_default(reveal_type(dct(X(), X())), reveal_type(dct(X(), X())))
    reveal_type(x7)  # revealed: X

    # revealed: dict[int | X, Any]
    # revealed: dict[str | X, Any]
    x8 = two_dicts_default(reveal_type({X(): X()}), reveal_type({X(): X()}))
    reveal_type(x8)  # revealed: X
```

## Prefer the declared type of generic classes and callables

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
reveal_type(f)  # revealed: list[Any] | None

g: list[Any] | dict[Any, Any] = f3(1)
reveal_type(g)  # revealed: list[Any] | dict[Any, Any]
```

When inferring a generic call, we only use the declared type as type context if it is in
non-covariant position. The final annotated assignment binding still uses the declared type if the
inferred and declared types are mutually assignable.

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

x5: Bivariant[int | None] = bivariant(1)
x6: Covariant[int | None] = covariant(1)
x7: Contravariant[int | None] = contravariant(1)
x8: Invariant[int | None] = invariant(1)

reveal_type(x5)  # revealed: Bivariant[int | None]
reveal_type(x6)  # revealed: Covariant[Literal[1]]
reveal_type(x7)  # revealed: Contravariant[int | None]
reveal_type(x8)  # revealed: Invariant[int | None]

x9: Bivariant[Any] = bivariant(1)
x10: Covariant[Any] = covariant(1)
x11: Contravariant[Any] = contravariant(1)
x12: Invariant[Any] = invariant(1)

reveal_type(x9)  # revealed: Bivariant[Any]
reveal_type(x10)  # revealed: Covariant[Any]
reveal_type(x11)  # revealed: Contravariant[Any]
reveal_type(x12)  # revealed: Invariant[Any]
```

```py
class X[T]:
    def __init__(self: X[None]): ...
    def pop(self) -> T:
        raise NotImplementedError

x1: X[int | None] = X()
reveal_type(x1)  # revealed: X[None]
```

We also prefer the declared type of `Callable` parameters, which are in contravariant position:

```py
from typing import Callable

type AnyToBool = Callable[[Any], bool]

def wrap[**P, T](f: Callable[P, T]) -> Callable[P, T]:
    return f

def make_callable[T](x: T) -> Callable[[T], bool]:
    raise NotImplementedError

def maybe_make_callable[T](x: T) -> Callable[[T], bool] | None:
    raise NotImplementedError

x1: Callable[[Any], bool] = make_callable(0)
reveal_type(x1)  # revealed: (Any, /) -> bool

x2: AnyToBool = make_callable(0)
reveal_type(x2)  # revealed: (Any, /) -> bool

x3: Callable[[list[Any]], bool] = make_callable([0])
reveal_type(x3)  # revealed: (list[Any], /) -> bool

x4: Callable[[Any], bool] = wrap(make_callable(0))
reveal_type(x4)  # revealed: (Any, /) -> bool

x5: Callable[[Any], bool] | None = maybe_make_callable(0)
reveal_type(x5)  # revealed: ((Any, /) -> bool) | None
```

## Declared type preference sees through subtyping

```toml
[environment]
python-version = "3.12"
```

Similarly, if the inferred type is a subtype of the declared type, we prefer declared type
assignments that are in non-covariant position.

```py
from collections import defaultdict
from typing import Any, Iterable, Literal, MutableSequence, Sequence

x1: Sequence[Any] = [1, 2, 3]
reveal_type(x1)  # revealed: list[int]

x2: MutableSequence[Any] = [1, 2, 3]
reveal_type(x2)  # revealed: list[Any]

x3: Iterable[Any] = [1, 2, 3]
reveal_type(x3)  # revealed: list[int]

x4: Iterable[Iterable[Any]] = [[1, 2, 3]]
reveal_type(x4)  # revealed: list[list[int]]

x5: list[Iterable[Any]] = [[1, 2, 3]]
reveal_type(x5)  # revealed: list[Iterable[Any]]

x6: Iterable[list[Any]] = [[1, 2, 3]]
reveal_type(x6)  # revealed: list[list[Any]]

x7: Sequence[Any] = [i for i in [1, 2, 3]]
reveal_type(x7)  # revealed: list[int]

x8: MutableSequence[Any] = [i for i in [1, 2, 3]]
reveal_type(x8)  # revealed: list[Any]

x9: Iterable[Any] = [i for i in [1, 2, 3]]
reveal_type(x9)  # revealed: list[int]

x10: Iterable[Iterable[Any]] = [[i] for i in [1, 2, 3]]
reveal_type(x10)  # revealed: list[list[int]]

x11: list[Iterable[Any]] = [[i] for i in [1, 2, 3]]
reveal_type(x11)  # revealed: list[Iterable[Any]]

x12: Iterable[list[Any]] = [[i] for i in [1, 2, 3]]
reveal_type(x12)  # revealed: list[list[Any]]

class X[T]:
    value: T

    def __init__(self, value: T): ...

class A[T](X[T]): ...

def a[T](value: T) -> A[T]:
    return A(value)

x13: A[object] = A(1)
reveal_type(x13)  # revealed: A[object]

x14: X[object] = A(1)
reveal_type(x14)  # revealed: A[object]

x15: X[object] | None = A(1)
reveal_type(x15)  # revealed: A[object]

x16: X[object] | None = a(1)
reveal_type(x16)  # revealed: A[object]

def f[T](x: T) -> list[list[T]]:
    return [[x]]

x17: Sequence[Sequence[Any]] = f(1)
reveal_type(x17)  # revealed: list[list[int]]

x18: Sequence[list[Any]] = f(1)
reveal_type(x18)  # revealed: list[list[Any]]

x19: dict[int, dict[str, int]] = defaultdict(dict)
reveal_type(x19)  # revealed: defaultdict[int, dict[str, int]]
```

## Narrow generic unions

```toml
[environment]
python-version = "3.12"
```

```py
from typing import reveal_type, Any, Callable, TypedDict

def identity[T](x: T) -> T:
    return x

type Target = Any | list[str] | dict[str, str] | Callable[[str], None] | None

def _(narrow: dict[str, str], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: dict[str, str]

def _(narrow: list[str], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: list[str]

def _(narrow: Callable[[str], None], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: (str, /) -> None

def _(narrow: list[str] | dict[str, str], target: Target):
    target = identity(narrow)
    reveal_type(target)  # revealed: list[str] | dict[str, str]

class TD(TypedDict):
    x: int

type TargetWithTD = Any | list[TD] | dict[str, TD] | Callable[[TD], None] | None

def _(target: TargetWithTD):
    target = identity([{"x": 1}])
    reveal_type(target)  # revealed: list[TD]

def _(target: TargetWithTD):
    target = identity({"x": {"x": 1}})
    reveal_type(target)  # revealed: dict[str, TD]

def _(target: TargetWithTD):
    def make_callable[T](x: T) -> Callable[[T], None]:
        raise NotImplementedError

    target = identity(make_callable({"x": 1}))
    reveal_type(target)  # revealed: (TD, /) -> None
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

    x6: str | int = f(lst(b))
    reveal_type(x6)  # revealed: str

    x7: str | None = f(lst(b))
    reveal_type(x7)  # revealed: str

    x8: int | str = f(lst(c))
    reveal_type(x8)  # revealed: int | str

    x9: int | str = f(lst(c))
    reveal_type(x9)  # revealed: int | str

    # TODO: Ideally this would reveal `int | str`. This is a known limitation of our
    # call inference solver, and would require an extra inference attempt without type
    # context, or with type context of subsets of the union, both of which are impractical
    # for performance reasons.
    x10: int | str | None = f(lst(c))
    reveal_type(x10)  # revealed: int | str | None
```

## Assignability diagnostics ignore declared type

```toml
[environment]
python-version = "3.12"
```

The type displayed in an invalid assignment diagnostic should account for the type context, e.g., to
avoid literal promotion:

```py
from typing import Literal, TypedDict

def f[T](x: T) -> list[T]:
    return [x]

# error: [invalid-assignment] "Object of type `list[Literal["hello"] | int]` is not assignable to `list[Literal["hello"] | bool]`"
x1: list[Literal["hello"] | bool] = ["hello", 1]

class A(TypedDict):
    bar: int

# error: [invalid-assignment] "Object of type `list[A | int]` is not assignable to `list[A | bool]`"
x2: list[A | bool] = [{"bar": 1}, 1]
```

However, the declared type should be ignored if the specialization is not solvable:

```py
from typing import Any, Callable

def g[T](x: list[T]) -> T:
    return x[0]

def _(a: int | None):
    # error: [invalid-assignment] "Object of type `list[int | None]` is not assignable to `list[str]`"
    x1: list[str] = f(a)

    # error: [invalid-assignment] "Object of type `int | None` is not assignable to `str`"
    x2: str = g(f(a))

def make_callable[T](x: T) -> Callable[[T], bool]:
    raise NotImplementedError

def _(a: int | None):
    # error: [invalid-assignment] "Object of type `(int | None, /) -> bool` is not assignable to `(str, /) -> bool`"
    x1: Callable[[str], bool] = make_callable(a)
```

## Forward annotation with unclosed string literal

Regression test for [#1611](https://github.com/astral-sh/ty/issues/1611).

<!-- fmt:off -->

```py
# error: [invalid-syntax]
# error: [invalid-syntax-in-forward-annotation]
a:'
```

<!-- fmt:on -->
