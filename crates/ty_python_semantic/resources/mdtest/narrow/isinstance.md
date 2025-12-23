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

## `classinfo` is a PEP-604 union of types

```toml
[environment]
python-version = "3.10"
```

```py
def _(x: int | str | bytes | memoryview | range):
    if isinstance(x, int | str):
        reveal_type(x)  # revealed: int | str
    elif isinstance(x, bytes | memoryview):
        reveal_type(x)  # revealed: bytes | memoryview[int]
    else:
        reveal_type(x)  # revealed: range
```

Although `isinstance()` usually only works if all elements in the `UnionType` are class objects, at
runtime a special exception is made for `None` so that `isinstance(x, int | None)` can work:

```py
def _(x: int | str | bytes | range | None):
    if isinstance(x, int | str | None):
        reveal_type(x)  # revealed: int | str | None
    else:
        reveal_type(x)  # revealed: bytes | range
```

## `classinfo` is an invalid PEP-604 union of types

Except for the `None` special case mentioned above, narrowing can only take place if all elements in
the PEP-604 union are class literals. If any elements are generic aliases or other types, the
`isinstance()` call may fail at runtime, so no narrowing can take place:

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.10"
```

```py
from typing import Any, Literal, NamedTuple

def _(x: int | list[int] | bytes):
    # error: [invalid-argument-type]
    if isinstance(x, list[int] | int):
        reveal_type(x)  # revealed: int | list[int] | bytes
    # error: [invalid-argument-type]
    elif isinstance(x, Literal[42] | list[int] | bytes):
        reveal_type(x)  # revealed: int | list[int] | bytes
    # error: [invalid-argument-type]
    elif isinstance(x, Any | NamedTuple | list[int]):
        reveal_type(x)  # revealed: int | list[int] | bytes
    else:
        reveal_type(x)  # revealed: int | list[int] | bytes
```

## PEP-604 unions on Python \<3.10

PEP-604 unions were added in Python 3.10, so attempting to use them on Python 3.9 does not lead to
any type narrowing.

```toml
[environment]
python-version = "3.9"
```

```py
def _(x: int | str | bytes):
    # error: [unsupported-operator]
    if isinstance(x, int | str):
        reveal_type(x)  # revealed: (int & Unknown) | (str & Unknown) | (bytes & Unknown)
    else:
        reveal_type(x)  # revealed: (int & Unknown) | (str & Unknown) | (bytes & Unknown)
```

## `classinfo` is a `types.UnionType`

Python 3.10 added the ability to use `Union[int, str]` as the second argument to `isinstance()`:

```py
from typing import Union

IntOrStr = Union[int, str]

reveal_type(IntOrStr)  # revealed: <types.UnionType special-form 'int | str'>

def _(x: int | str | bytes | memoryview | range):
    if isinstance(x, IntOrStr):
        reveal_type(x)  # revealed: int | str
    elif isinstance(x, Union[bytes, memoryview]):
        reveal_type(x)  # revealed: bytes | memoryview[int]
    else:
        reveal_type(x)  # revealed: range

def _(x: int | str | None):
    if isinstance(x, Union[int, None]):
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: str

ListStrOrInt = Union[list[str], int]

def _(x: dict[int, str] | ListStrOrInt):
    # TODO: this should ideally be an error
    if isinstance(x, ListStrOrInt):
        # TODO: this should not be narrowed
        reveal_type(x)  # revealed: list[str] | int

    # TODO: this should ideally be an error
    if isinstance(x, Union[list[str], int]):
        # TODO: this should not be narrowed
        reveal_type(x)  # revealed: list[str] | int
```

## `Optional` as `classinfo`

```py
from typing import Optional

def _(x: int | str | None):
    if isinstance(x, Optional[int]):
        reveal_type(x)  # revealed: int | None
    else:
        reveal_type(x)  # revealed: str
```

## `classinfo` is a `typing.py` special form

Certain special forms in `typing.py` are aliases to classes elsewhere in the standard library; these
can be used in `isinstance()` and `issubclass()` checks. We support narrowing using them:

```py
import typing as t

def f(x: dict[str, int] | list[str], y: object):
    if isinstance(x, t.Dict):
        reveal_type(x)  # revealed: dict[str, int]
    else:
        reveal_type(x)  # revealed: list[str]

    if isinstance(y, t.Callable):
        reveal_type(y)  # revealed: Top[(...) -> object]
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

    # error: [invalid-argument-type] "Argument to function `isinstance` is incorrect: Expected `type | UnionType | tuple[Divergent, ...]`, found `Literal["a"]"
    if isinstance(x, "a"):
        reveal_type(x)  # revealed: Literal[1, "a"]

    # error: [invalid-argument-type] "Argument to function `isinstance` is incorrect: Expected `type | UnionType | tuple[Divergent, ...]`, found `Literal["int"]"
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

## Generic aliases are not supported as second argument

The `classinfo` argument cannot be a generic alias:

```py
def _(x: list[str] | list[int] | list[bytes]):
    # TODO: Ideally, this would be an error (requires https://github.com/astral-sh/ty/issues/116)
    if isinstance(x, list[int]):
        # No narrowing here:
        reveal_type(x)  # revealed: list[str] | list[int] | list[bytes]

    # error: [invalid-argument-type] "Invalid second argument to `isinstance`"
    if isinstance(x, list[int] | list[str]):
        # No narrowing here:
        reveal_type(x)  # revealed: list[str] | list[int] | list[bytes]
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

## Narrowing with generics

```toml
[environment]
python-version = "3.12"
```

Narrowing to a generic class using `isinstance()` uses the top materialization of the generic. With
a covariant generic, this is equivalent to using the upper bound of the type parameter (by default,
`object`):

```py
from typing import Self

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def _(x: object):
    if isinstance(x, Covariant):
        reveal_type(x)  # revealed: Covariant[object]
        reveal_type(x.get())  # revealed: object
```

Similarly, contravariant type parameters use their lower bound of `Never`:

```py
class Contravariant[T]:
    def push(self, x: T) -> None: ...

def _(x: object):
    if isinstance(x, Contravariant):
        reveal_type(x)  # revealed: Contravariant[Never]
        # error: [invalid-argument-type] "Argument to bound method `push` is incorrect: Expected `Never`, found `Literal[42]`"
        x.push(42)
```

Invariant generics are trickiest. The top materialization, conceptually the type that includes all
instances of the generic class regardless of the type parameter, cannot be represented directly in
the type system, so we represent it with the internal `Top[]` special form.

```py
class Invariant[T]:
    def push(self, x: T) -> None: ...
    def get(self) -> T:
        raise NotImplementedError

def _(x: object):
    if isinstance(x, Invariant):
        reveal_type(x)  # revealed: Top[Invariant[Unknown]]
        reveal_type(x.get())  # revealed: object
        # error: [invalid-argument-type] "Argument to bound method `push` is incorrect: Expected `Never`, found `Literal[42]`"
        x.push(42)
```

When more complex types are involved, the `Top[]` type may get simplified away.

```py
def _(x: list[int] | set[str]):
    if isinstance(x, list):
        reveal_type(x)  # revealed: list[int]
    else:
        reveal_type(x)  # revealed: set[str]
```

Though if the types involved are not disjoint bases, we necessarily keep a more complex type.

```py
def _(x: Invariant[int] | Covariant[str]):
    if isinstance(x, Invariant):
        reveal_type(x)  # revealed: Invariant[int] | (Covariant[str] & Top[Invariant[Unknown]])
    else:
        reveal_type(x)  # revealed: Covariant[str] & ~Top[Invariant[Unknown]]
```

The behavior of `issubclass()` is similar.

```py
def _(x: type[object], y: type[object], z: type[object]):
    if issubclass(x, Covariant):
        reveal_type(x)  # revealed: type[Covariant[object]]
    if issubclass(y, Contravariant):
        reveal_type(y)  # revealed: type[Contravariant[Never]]
    if issubclass(z, Invariant):
        reveal_type(z)  # revealed: type[Top[Invariant[Unknown]]]
```
