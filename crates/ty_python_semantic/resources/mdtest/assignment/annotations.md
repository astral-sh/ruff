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

class C:
    declared: int

c = C()
# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `C`."
# error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
c.unresolved: int = "foo"
# error: [invalid-assignment] "Attribute `declared` was declared as type `int` in the class body, but here it is declared as the incompatible type `str`"
c.declared: str = "foo"

def f() -> C:
    return C()

# This is not a definition we track.
# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `C`."
# error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
f().unresolved: int = "foo"

l = []
# error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"
l[0]: int = "foo"
```

## Violates previous annotation

```py
x: int
x = "foo"  # error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to `int`"

class C:
    declared: int

c = C()
# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `C`."
c.unresolved: int
# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `C`."
c.unresolved = "foo"
# error: [invalid-assignment] "Attribute `declared` was declared as type `int` in the class body, but here it is declared as the incompatible type `str`"
c.declared: str
# error: [invalid-assignment] "Object of type `Literal["foo"]` is not assignable to attribute `declared` of type `int`"
c.declared = "foo"

def f() -> C:
    return C()

# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `C`."
f().unresolved: int
# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `C`."
f().unresolved = "foo"

# TODO: This doesn't cause a type error in mypy or pyright, but it might be better to treat it as an error.
l = []
l[0]: int
l[0] = "foo"
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

reveal_type(f)  # revealed: @Todo(PEP 646)
reveal_type(g)  # revealed: @Todo(PEP 646)

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

# error: [invalid-assignment] "Object of type `tuple[list[Unknown], Literal["foo"]]` is not assignable to `tuple[str | int, str]`"
c: tuple[str | int, str] = ([], "foo")

class D:
    declared: tuple[int, int]

d = D()
# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `D`."
# error: [invalid-assignment] "Object of type `tuple[Literal[1], Literal[2]]` is not assignable to `tuple[()]`"
d.unresolved: tuple[()] = (1, 2)
# error: [invalid-assignment] "Attribute `declared` was declared as type `tuple[int, int]` in the class body, but here it is declared as the incompatible type `tuple[()]`"
# error: [invalid-assignment] "Object of type `tuple[Literal[1], Literal[2]]` is not assignable to `tuple[()]`"
d.declared: tuple[()] = (1, 2)

def f() -> D:
    return D()

# error: [unresolved-attribute] "Unresolved attribute `unresolved` on type `D`."
# error: [invalid-assignment] "Object of type `tuple[Literal[1], Literal[2]]` is not assignable to `tuple[()]`"
f().unresolved: tuple[()] = (1, 2)

l = []
# error: [invalid-assignment] "Object of type `tuple[Literal[1], Literal[2]]` is not assignable to `tuple[()]`"
l[0]: tuple[()] = (1, 2)
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
