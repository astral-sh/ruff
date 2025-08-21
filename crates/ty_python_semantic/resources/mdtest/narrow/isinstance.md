# Narrowing for `isinstance` checks

Narrowing for `isinstance(object, classinfo)` expressions.

## `classinfo` is a single type

```py
def _(flag: bool):
    x = 1 if flag else "a"

    if isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1]

    if isinstance(x, str):
        reveal_type(x)  # revealed: Literal["a"]
        if isinstance(x, int):
            reveal_type(x)  # revealed: Never

    if isinstance(x, (int, object)):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## `classinfo` is a tuple of types

Note: `isinstance(x, (int, str))` should not be confused with `isinstance(x, tuple[(int, str)])`.
The former is equivalent to `isinstance(x, int | str)`:

```py
def _(flag: bool, flag1: bool, flag2: bool):
    x = 1 if flag else "a"

    if isinstance(x, (int, str)):
        reveal_type(x)  # revealed: Literal[1, "a"]
    else:
        reveal_type(x)  # revealed: Never

    if isinstance(x, (int, bytes)):
        reveal_type(x)  # revealed: Literal[1]

    if isinstance(x, (bytes, str)):
        reveal_type(x)  # revealed: Literal["a"]

    # No narrowing should occur if a larger type is also
    # one of the possibilities:
    if isinstance(x, (int, object)):
        reveal_type(x)  # revealed: Literal[1, "a"]
    else:
        reveal_type(x)  # revealed: Never

    y = 1 if flag1 else "a" if flag2 else b"b"
    if isinstance(y, (int, str)):
        reveal_type(y)  # revealed: Literal[1, "a"]

    if isinstance(y, (int, bytes)):
        reveal_type(y)  # revealed: Literal[1, b"b"]

    if isinstance(y, (str, bytes)):
        reveal_type(y)  # revealed: Literal["a", b"b"]
```

## `classinfo` is a nested tuple of types

```py
def _(flag: bool):
    x = 1 if flag else "a"

    if isinstance(x, (bool, (bytes, int))):
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: Literal["a"]
```

## Class types

```py
class A: ...
class B: ...
class C: ...

x = object()

if isinstance(x, A):
    reveal_type(x)  # revealed: A
    if isinstance(x, B):
        reveal_type(x)  # revealed: A & B
    else:
        reveal_type(x)  # revealed: A & ~B

if isinstance(x, (A, B)):
    reveal_type(x)  # revealed: A | B
elif isinstance(x, (A, C)):
    reveal_type(x)  # revealed: C & ~A & ~B
else:
    reveal_type(x)  # revealed: ~A & ~B & ~C
```

## No narrowing for instances of `builtins.type`

```py
def _(flag: bool, t: type):
    x = 1 if flag else "foo"

    if isinstance(x, t):
        reveal_type(x)  # revealed: Literal[1, "foo"]
```

## Do not use custom `isinstance` for narrowing

```py
def _(flag: bool):
    def isinstance(x, t):
        return True
    x = 1 if flag else "a"

    if isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Do support narrowing if `isinstance` is aliased

```py
def _(flag: bool):
    isinstance_alias = isinstance

    x = 1 if flag else "a"

    if isinstance_alias(x, int):
        reveal_type(x)  # revealed: Literal[1]
```

## Do support narrowing if `isinstance` is imported

```py
from builtins import isinstance as imported_isinstance

def _(flag: bool):
    x = 1 if flag else "a"

    if imported_isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1]
```

## Do not narrow if second argument is not a type

```py
def _(flag: bool):
    x = 1 if flag else "a"

    # TODO: this should cause us to emit a diagnostic during
    # type checking
    if isinstance(x, "a"):
        reveal_type(x)  # revealed: Literal[1, "a"]

    # TODO: this should cause us to emit a diagnostic during
    # type checking
    if isinstance(x, "int"):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Do not narrow if there are keyword arguments

```py
def _(flag: bool):
    x = 1 if flag else "a"

    # error: [unknown-argument]
    if isinstance(x, int, foo="bar"):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## `type[]` types are narrowed as well as class-literal types

```py
def _(x: object, y: type[int]):
    if isinstance(x, y):
        reveal_type(x)  # revealed: int
```

## Adding a disjoint element to an existing intersection

We used to incorrectly infer `Literal` booleans for some of these.

```py
from ty_extensions import Not, Intersection, AlwaysTruthy, AlwaysFalsy

class P: ...

def f(
    a: Intersection[P, AlwaysTruthy],
    b: Intersection[P, AlwaysFalsy],
    c: Intersection[P, Not[AlwaysTruthy]],
    d: Intersection[P, Not[AlwaysFalsy]],
):
    if isinstance(a, bool):
        reveal_type(a)  # revealed: Never
    else:
        reveal_type(a)  # revealed: P & AlwaysTruthy

    if isinstance(b, bool):
        reveal_type(b)  # revealed: Never
    else:
        reveal_type(b)  # revealed: P & AlwaysFalsy

    if isinstance(c, bool):
        reveal_type(c)  # revealed: Never
    else:
        reveal_type(c)  # revealed: P & ~AlwaysTruthy

    if isinstance(d, bool):
        reveal_type(d)  # revealed: Never
    else:
        reveal_type(d)  # revealed: P & ~AlwaysFalsy
```

## Narrowing if an object of type `Any` or `Unknown` is used as the second argument

In order to preserve the gradual guarantee, we intersect with the type of the second argument if the
type of the second argument is a dynamic type:

```py
from typing import Any
from something_unresolvable import SomethingUnknown  # error: [unresolved-import]

class Foo: ...

def f(a: Foo, b: Any):
    if isinstance(a, SomethingUnknown):
        reveal_type(a)  # revealed: Foo & Unknown

    if isinstance(a, b):
        reveal_type(a)  # revealed: Foo & Any
```

## Narrowing if an object with an intersection/union/TypeVar type is used as the second argument

If an intersection with only positive members is used as the second argument, and all positive
members of the intersection are valid arguments for the second argument to `isinstance()`, we
intersect with each positive member of the intersection:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any
from ty_extensions import Intersection

class Foo: ...

class Bar:
    attribute: int

class Baz:
    attribute: str

def f(x: Foo, y: Intersection[type[Bar], type[Baz]], z: type[Any]):
    if isinstance(x, y):
        reveal_type(x)  # revealed: Foo & Bar & Baz

    if isinstance(x, z):
        reveal_type(x)  # revealed: Foo & Any
```

The same if a union type is used:

```py
def g(x: Foo, y: type[Bar | Baz]):
    if isinstance(x, y):
        reveal_type(x)  # revealed: (Foo & Bar) | (Foo & Baz)
```

And even if a `TypeVar` is used, providing it has valid upper bounds/constraints:

```py
from typing import TypeVar

T = TypeVar("T", bound=type[Bar])

def h_old_syntax(x: Foo, y: T) -> T:
    if isinstance(x, y):
        reveal_type(x)  # revealed: Foo & Bar
        reveal_type(x.attribute)  # revealed: int

    return y

def h[U: type[Bar | Baz]](x: Foo, y: U) -> U:
    if isinstance(x, y):
        reveal_type(x)  # revealed: (Foo & Bar) | (Foo & Baz)
        reveal_type(x.attribute)  # revealed: int | str

    return y
```

Or even a tuple of tuple of typevars that have intersection bounds...

```py
from ty_extensions import Intersection

class Spam: ...
class Eggs: ...
class Ham: ...
class Mushrooms: ...

def i[T: Intersection[type[Bar], type[Baz | Spam]], U: (type[Eggs], type[Ham])](x: Foo, y: T, z: U) -> tuple[T, U]:
    if isinstance(x, (y, (z, Mushrooms))):
        reveal_type(x)  # revealed: (Foo & Bar & Baz) | (Foo & Bar & Spam) | (Foo & Eggs) | (Foo & Ham) | (Foo & Mushrooms)

    return (y, z)
```
