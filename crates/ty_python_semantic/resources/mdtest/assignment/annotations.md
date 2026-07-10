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

## PEP-604 annotations are supported

```py
def foo(v: str | int | None, w: str | str | None, x: str | str):
    reveal_type(v)  # revealed: str | int | None
    reveal_type(w)  # revealed: str | None
    reveal_type(x)  # revealed: str
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

## Forward annotation with unclosed string literal

Regression test for [#1611](https://github.com/astral-sh/ty/issues/1611).

<!-- fmt:off -->

```py
# error: [invalid-syntax]
# error: [invalid-syntax-in-forward-annotation]
a:'
```

<!-- fmt:on -->
