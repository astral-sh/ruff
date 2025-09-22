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
reveal_type(i)  # revealed: list[Any | int | str | tuple[list[Unknown | int]]]

j: list[tuple[str | int, ...]] = [(1, 2), ("foo", "bar"), ()]
reveal_type(j)  # revealed: list[tuple[str | int, ...]]

k: list[tuple[list[int], ...]] = [([],), ([1, 2], [3, 4]), ([5], [6], [7])]
reveal_type(k)  # revealed: list[tuple[list[int], ...]]

l: tuple[list[int], *tuple[list[typing.Any], ...], list[str]] = ([1, 2, 3], [4, 5, 6], [7, 8, 9], ["10", "11", "12"])
reveal_type(l)  # revealed: tuple[list[int], list[Any | int], list[Any | int], list[str]]

type IntList = list[int]

m: IntList = [1, 2, 3]
reveal_type(m)  # revealed: list[int]

# TODO: this should type-check and avoid literal promotion
# error: [invalid-assignment] "Object of type `list[Unknown | int]` is not assignable to `list[Literal[1, 2, 3]]`"
n: list[typing.Literal[1, 2, 3]] = [1, 2, 3]
reveal_type(n)  # revealed: list[Literal[1, 2, 3]]

# TODO: this should type-check and avoid literal promotion
# error: [invalid-assignment] "Object of type `list[Unknown | str]` is not assignable to `list[LiteralString]`"
o: list[typing.LiteralString] = ["a", "b", "c"]
reveal_type(o)  # revealed: list[LiteralString]

p: dict[int, int] = {}
reveal_type(p)  # revealed: dict[int, int]

q: dict[int | str, int] = {1: 1, 2: 2, 3: 3}
reveal_type(q)  # revealed: dict[int | str, int]

r: dict[int | str, int | str] = {1: 1, 2: 2, 3: 3}
reveal_type(r)  # revealed: dict[int | str, int | str]
```

## Incorrect collection literal assignments are complained aobut

```py
# error: [invalid-assignment] "Object of type `list[Unknown | int]` is not assignable to `list[str]`"
a: list[str] = [1, 2, 3]

# error: [invalid-assignment] "Object of type `set[Unknown | int | str]` is not assignable to `set[int]`"
b: set[int] = {1, 2, "3"}
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

```py
from typing import Literal

def f[T](x: T) -> list[T]:
    return [x]

a = f("a")
reveal_type(a)  # revealed: list[Literal["a"]]

b: list[int | Literal["a"]] = f("a")
reveal_type(b)  # revealed: list[int | Literal["a"]]

c: list[int | str] = f("a")
reveal_type(c)  # revealed: list[int | str]

d: list[int | tuple[int, int]] = f((1, 2))
reveal_type(d)  # revealed: list[int | tuple[int, int]]

e: list[int] = f(True)
reveal_type(e)  # revealed: list[int]

# error: [invalid-assignment] "Object of type `list[Literal["a"]]` is not assignable to `list[int]`"
g: list[int] = f("a")

# error: [invalid-assignment] "Object of type `list[Literal["a"]]` is not assignable to `tuple[int]`"
h: tuple[int] = f("a")

def f2[T: int](x: T) -> T:
    return x

i: int = f2(True)
reveal_type(i)  # revealed: int

j: int | str = f2(True)
reveal_type(j)  # revealed: Literal[True]
```
