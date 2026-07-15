# Narrowing for `isinstance` checks

Narrowing for `isinstance(object, classinfo)` expressions.

## `classinfo` is a single type

```py
from typing import Literal

def _(x: Literal[1, "a"]):
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
from typing import Literal

def _(x: Literal[1, "a"], y: Literal[1, "a", b"b"]):
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

    if isinstance(y, (int, str)):
        reveal_type(y)  # revealed: Literal[1, "a"]

    if isinstance(y, (int, bytes)):
        reveal_type(y)  # revealed: Literal[1, b"b"]

    if isinstance(y, (str, bytes)):
        reveal_type(y)  # revealed: Literal["a", b"b"]
```

## `classinfo` is a nested tuple of types

```py
from typing import Literal

def _(x: Literal[1, "a"]):
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

```toml
[environment]
python-version = "3.10"
```

```py
from typing import Any, Literal, NamedTuple

def _(x: int | list[int] | bytes):
    # snapshot: invalid-argument-type
    if isinstance(x, list[int] | int):
        reveal_type(x)  # revealed: int | list[int] | bytes
```

```snapshot
error[invalid-argument-type]: Invalid second argument to `isinstance`
 --> src/mdtest_snippet.py:5:8
  |
5 |     if isinstance(x, list[int] | int):
  |        ^^^^^^^^^^^^^^---------------^
  |                      |
  |                      This `UnionType` instance contains non-class elements
  |
info: A `UnionType` instance can only be used as the second argument to `isinstance` if all elements are class objects
info: Element `<class 'list[int]'>` in the union is not a class object
```

```py
    # snapshot: invalid-argument-type
    elif isinstance(x, Literal[42] | list[int] | bytes):
        reveal_type(x)  # revealed: int | list[int] | bytes
```

```snapshot
error[invalid-argument-type]: Invalid second argument to `isinstance`
 --> src/mdtest_snippet.py:8:10
  |
8 |     elif isinstance(x, Literal[42] | list[int] | bytes):
  |          ^^^^^^^^^^^^^^-------------------------------^
  |                        |
  |                        This `UnionType` instance contains non-class elements
  |
info: A `UnionType` instance can only be used as the second argument to `isinstance` if all elements are class objects
info: Elements `<special-form 'Literal[42]'>` and `<class 'list[int]'>` in the union are not class objects
```

```py
    # snapshot: invalid-argument-type
    elif isinstance(x, Any | NamedTuple | list[int]):
        reveal_type(x)  # revealed: int | list[int] | bytes
```

```snapshot
error[invalid-argument-type]: Invalid second argument to `isinstance`
  --> src/mdtest_snippet.py:11:10
   |
11 |     elif isinstance(x, Any | NamedTuple | list[int]):
   |          ^^^^^^^^^^^^^^----------------------------^
   |                        |
   |                        This `UnionType` instance contains non-class elements
   |
info: A `UnionType` instance can only be used as the second argument to `isinstance` if all elements are class objects
info: Element `<special-form 'typing.Any'>` in the union, and 2 more elements, are not class objects
```

```py
    else:
        reveal_type(x)  # revealed: int | list[int] | bytes
```

The same validation also applies when an invalid `UnionType` is nested inside a tuple:

```py
def _(x: int | list[int] | bytes):
    # snapshot: invalid-argument-type
    if isinstance(x, (int, list[int] | bytes)):
        reveal_type(x)  # revealed: int | list[int] | bytes
    else:
        reveal_type(x)  # revealed: int | list[int] | bytes
```

```snapshot
error[invalid-argument-type]: Invalid second argument to `isinstance`
  --> src/mdtest_snippet.py:17:8
   |
17 |     if isinstance(x, (int, list[int] | bytes)):
   |        ^^^^^^^^^^^^^^^^^^^^-----------------^^
   |                            |
   |                            This `UnionType` instance contains non-class elements
   |
info: A `UnionType` instance can only be used as the second argument to `isinstance` if all elements are class objects
info: Element `<class 'list[int]'>` in the union is not a class object
```

Including nested tuples:

```py
def _(x: int | list[int] | bytes):
    # snapshot: invalid-argument-type
    if isinstance(x, (int, (str, list[int] | bytes))):
        reveal_type(x)  # revealed: int | list[int] | bytes
    else:
        reveal_type(x)  # revealed: int | list[int] | bytes
```

```snapshot
error[invalid-argument-type]: Invalid second argument to `isinstance`
  --> src/mdtest_snippet.py:23:8
   |
23 |     if isinstance(x, (int, (str, list[int] | bytes))):
   |        ^^^^^^^^^^^^^^^^^^^^^^^^^^-----------------^^^
   |                                  |
   |                                  This `UnionType` instance contains non-class elements
   |
info: A `UnionType` instance can only be used as the second argument to `isinstance` if all elements are class objects
info: Element `<class 'list[int]'>` in the union is not a class object
```

And non-literal tuples:

```py
classes = (int, list[int] | bytes)

def _(x: int | list[int] | bytes):
    # snapshot: invalid-argument-type
    if isinstance(x, classes):
        reveal_type(x)  # revealed: int | list[int] | bytes
    else:
        reveal_type(x)  # revealed: int | list[int] | bytes
```

```snapshot
error[invalid-argument-type]: Invalid second argument to `isinstance`
  --> src/mdtest_snippet.py:31:8
   |
31 |     if isinstance(x, classes):
   |        ^^^^^^^^^^^^^^^^^^^^^^
   |
info: A `UnionType` instance can only be used as the second argument to `isinstance` if all elements are class objects
info: Element `<class 'list[int]'>` in the union `list[int] | bytes` is not a class object
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
from typing import Literal

def _(x: Literal[1, "foo"], t: type):
    if isinstance(x, t):
        reveal_type(x)  # revealed: Literal[1, "foo"]
```

## Do not use custom `isinstance` for narrowing

```py
from typing import Literal

def _(x: Literal[1, "a"]):
    def isinstance(x, t):
        return True

    if isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Do support narrowing if `isinstance` is aliased

```py
from typing import Literal

def _(x: Literal[1, "a"]):
    isinstance_alias = isinstance

    if isinstance_alias(x, int):
        reveal_type(x)  # revealed: Literal[1]
```

## Do support narrowing if `isinstance` is imported

```py
from builtins import isinstance as imported_isinstance
from typing import Literal

def _(x: Literal[1, "a"]):
    if imported_isinstance(x, int):
        reveal_type(x)  # revealed: Literal[1]
```

## Do not narrow if second argument is not a type

```py
from typing import Literal

def _(x: Literal[1, "a"]):
    # error: [invalid-argument-type] "Argument to function `isinstance` is incorrect: Expected `type | UnionType | tuple[Divergent, ...]`, found `Literal["a"]"
    if isinstance(x, "a"):
        reveal_type(x)  # revealed: Literal[1, "a"]

    # error: [invalid-argument-type] "Argument to function `isinstance` is incorrect: Expected `type | UnionType | tuple[Divergent, ...]`, found `Literal["int"]"
    if isinstance(x, "int"):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Do not narrow if there are keyword arguments

```py
from typing import Literal

def _(x: Literal[1, "a"]):
    # error: [unknown-argument]
    if isinstance(x, int, foo="bar"):
        reveal_type(x)  # revealed: Literal[1, "a"]
```

## Splatted calls with invalid `classinfo`

Diagnostics are still emitted for invalid `classinfo` types when the arguments are splatted:

```py
args = (object(), int | list[str])
isinstance(*args)  # error: [invalid-argument-type]
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

Negative narrowing is not sound in this case, because `type[A]` includes subclasses of `A`:

```py
class A: ...
class B: ...

def f(x: A | B, y: type[A]):
    if isinstance(x, y):
        reveal_type(x)  # revealed: A
        return

    reveal_type(x)  # revealed: A | B
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

If some (but not all) positive members of the intersection are not valid `isinstance()` targets --
for example a parametrized generic alias such as `type[list[int]]`, which raises `TypeError` at
runtime -- we skip those members and narrow using the remaining valid ones, rather than declining to
narrow at all:

```py
from ty_extensions import Intersection

def f(x: Foo, y: Intersection[type[Bar], type[list[int]]]):
    if isinstance(x, y):
        # `type[list[int]]` is not a valid `isinstance()` target and contributes no
        # constraint, but `type[Bar]` still narrows.
        reveal_type(x)  # revealed: Foo & Bar
        reveal_type(x.attribute)  # revealed: int
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
        # error: [invalid-argument-type] "Argument to bound method `Contravariant.push` is incorrect: Expected `Never`, found `Literal[42]`"
        x.push(42)
```

The same applies when the contravariant type parameter appears inside `type[T]`:

```py
from typing import Generic, TypeVar

T = TypeVar("T", contravariant=True)

class ContravariantType(Generic[T]):
    def push(self, x: type[T]) -> None: ...

def _(x: object):
    if isinstance(x, ContravariantType):
        reveal_type(x)  # revealed: ContravariantType[Never]
        # error: [invalid-argument-type]
        x.push(str)
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
        # error: [invalid-argument-type] "Argument to bound method `Invariant.push` is incorrect: Expected `Never`, found `Literal[42]`"
        x.push(42)
```

When reading attributes from a top-materialized generic, only type parameters should be
materialized. Unrelated gradual attribute types should be preserved.

```py
from typing import Any

class InvariantWithAny[T: int]:
    a: T
    b: Any

def _(x: object):
    if isinstance(x, InvariantWithAny):
        reveal_type(x)  # revealed: Top[InvariantWithAny[Unknown]]
        reveal_type(x.a)  # revealed: object
        reveal_type(x.b)  # revealed: Any
```

The same applies in contravariant positions: `Any` in a parameter type that isn't tied to the
generic parameter should not be materialized.

```py
from typing import Any

class ContravariantWithAny[T]:
    def push(self, x: T, y: Any) -> None: ...

def _(x: object):
    if isinstance(x, ContravariantWithAny):
        reveal_type(x)  # revealed: ContravariantWithAny[Never]
        # error: [invalid-argument-type] "Argument to bound method `ContravariantWithAny.push` is incorrect: Expected `Never`, found `Literal[42]`"
        x.push(42, "hello")
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

## Narrowing `TypedDict`s to runtime dictionary classes

A `TypedDict` object is a `dict` and a `MutableMapping` at runtime, even though its static interface
does not allow arbitrary mutation. Narrowing from `object` must therefore allow both ordinary
dictionaries and any `TypedDict`:

```py
from collections.abc import MutableMapping
from typing import TypedDict
from typing_extensions import Never

class Movie(TypedDict):
    title: str

def narrow_object(value: object) -> None:
    if isinstance(value, dict):
        reveal_type(value)  # revealed: Top[dict[Unknown, Unknown]] | <TypedDict with no items>

    if isinstance(value, MutableMapping):
        reveal_type(value)  # revealed: Top[MutableMapping[Unknown, Unknown]] | <TypedDict with no items>

def narrow_movie(movie: Movie) -> None:
    if isinstance(movie, MutableMapping):
        reveal_type(movie)  # revealed: Movie
        reveal_type(reversed(movie))  # revealed: Iterator[str]
```

For an unconstrained type variable, neither the ordinary dictionary possibility nor the `TypedDict`
possibility can be discarded:

```py
from typing import TypeVar

TDict = TypeVar("TDict")

def _(value: Movie | TDict):
    if isinstance(value, dict):
        reveal_type(value)  # revealed: Movie | (TDict@_ & Top[dict[Unknown, Unknown]]) | (TDict@_ & <TypedDict with no items>)
```

The possibility of a `TypedDict` preserves read-only dictionary operations without exposing
arbitrary mutation:

```py
def takes_dict(value: dict[str, object]) -> None: ...
def use_narrowed_dict(
    value: object,
    key: object,
    missing: Never,
    keys: list[str],
    item: int,
) -> None:
    if isinstance(value, dict):
        reveal_type(value.get(key))  # revealed: object
        reveal_type(reversed(value))  # revealed: Iterator[object]
        reveal_type(value.fromkeys(keys, item))  # revealed: dict[str, int]
        reveal_type(value.copy())  # revealed: Top[dict[Unknown, Unknown]] | <TypedDict with no items>
        value.clear()  # error: [unresolved-attribute]
        value.pop(missing)
        value.setdefault(missing, missing)
        takes_dict(value)  # error: [invalid-argument-type]
```

Merging two values narrowed to `dict` produces an ordinary dictionary. Calling the corresponding
special methods directly has the same result:

```py
def merge_narrowed_dicts(left: object, right: object) -> None:
    if isinstance(left, dict) and isinstance(right, dict):
        reveal_type(left | right)  # revealed: dict[Unknown, Unknown]
        reveal_type(left.__or__(right))  # revealed: dict[Unknown, Unknown]
        reveal_type(right.__ror__(left))  # revealed: dict[Unknown, Unknown]
```

Narrowing a union with a concrete dictionary keeps its methods callable with `IntEnum` keys:

```py
from enum import IntEnum
from typing import Protocol

class DiagnosticField(IntEnum):
    MESSAGE = 77

class PGresult(Protocol):
    def error_field(self, fieldcode: int) -> bytes | None: ...

ErrorInfo = PGresult | dict[int, bytes | None] | None

def _(info: ErrorInfo):
    if isinstance(info, dict):
        reveal_type(info.get(DiagnosticField.MESSAGE))  # revealed: object
    elif info:
        reveal_type(info.error_field(DiagnosticField.MESSAGE))  # revealed: bytes | None
```

Runtime protocol checks preserve `TypedDict` possibilities by checking the concrete `dict` member
surface. Compatible protocols retain their structural refinement, while an incompatible static
signature cannot eliminate a runtime-compatible `TypedDict`:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasClear(Protocol):
    def clear(self) -> None: ...

@runtime_checkable
class HasKeysReturningInt(Protocol):
    def keys(self) -> int: ...

@runtime_checkable
class HasMissingMember(Protocol):
    def missing(self) -> None: ...

@runtime_checkable
class HasHash(Protocol):
    def __hash__(self) -> int: ...
    def keys(self) -> object: ...

def preserve_empty_typed_dict_protocol(value: object) -> None:
    if isinstance(value, dict) and isinstance(value, HasClear):
        reveal_type(value)  # revealed: Top[dict[Unknown, Unknown]] | (<TypedDict with no items> & HasClear)
        value.clear()

def runtime_protocols_use_dict_member_presence(movie: Movie) -> None:
    if isinstance(movie, HasKeysReturningInt):
        # Runtime protocol checks ignore the incompatible static return type.
        reveal_type(movie)  # revealed: Movie
    else:
        reveal_type(movie)  # revealed: Never

    if isinstance(movie, HasMissingMember):
        reveal_type(movie)  # revealed: Never
    else:
        reveal_type(movie)  # revealed: Movie

    if isinstance(movie, HasHash):
        # `dict.__hash__` exists but is explicitly disabled with `None`.
        reveal_type(movie)  # revealed: Never
    else:
        reveal_type(movie)  # revealed: Movie
```

`TypedDict` objects have exact runtime type `dict`, so they cannot be instances of a proper `dict`
subclass:

```py
from collections import defaultdict

def narrow_typed_dict_defaultdict(movie: Movie) -> None:
    if isinstance(movie, defaultdict):
        reveal_type(movie)  # revealed: Never
```

## Narrowing generic defaults in Python 3.13

When a type parameter has a bare `Any` default, narrowing still materializes the substituted
typevar. The default isn't used during `isinstance` narrowing (the type parameter gets `Unknown`
instead), so the default value is irrelevant here:

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any

class WithAnyDefault[T = Any]:
    y: tuple[Any, T]

def _(x: object):
    if isinstance(x, WithAnyDefault):
        reveal_type(x.y)  # revealed: tuple[Any, object]
```

Type alias defaults substituted into type parameters still need to be materialized when narrowing:

```py
from typing import Any

type A = Any

class WithAliasDefault[T = A]:
    y: tuple[A, T]

def _(x: object):
    if isinstance(x, WithAliasDefault):
        reveal_type(x.y)  # revealed: tuple[A, object]
```

## Narrowing generic `classmethod`

After an `isinstance(..., classmethod)` branch unwraps and replaces a generic `classmethod`, the
false-branch residual should be impossible. This avoids retaining a `classmethod[...] & Top[...]`
arm that later causes `call-top-callable` false positives.

```toml
[environment]
python-version = "3.13"
```

```py
from collections.abc import Callable
from typing import Any, ParamSpec, TypeVar, cast

P = ParamSpec("P")
R = TypeVar("R")

def f(fn: Callable[P, R] | classmethod[Any, P, R]) -> Callable[P, R]:
    if isinstance(fn, classmethod):
        fn = cast(Callable[P, R], fn.__func__)

    if not callable(fn):
        raise TypeError

    reveal_type(fn)  # revealed: (**P@f) -> R@f
    return fn
```

## Narrowing with TypedDict unions

`TypedDict` unions narrow through `isinstance(x, dict)` without leaving intersections with `dict`.
This also covers the previous panic regression from <https://github.com/astral-sh/ty/issues/2451>.

```py
from typing import TypedDict

class A(TypedDict):
    x: str

class B(TypedDict):
    y: str

T = int | A | B

def narrow_typeddict_union(v: T) -> None:
    if isinstance(v, dict):
        reveal_type(v)  # revealed: A | B
    else:
        reveal_type(v)  # revealed: int
```

This also covers the recursive `Result | list[Result]` example reported on
<https://github.com/astral-sh/ty/issues/1130#issuecomment-4762266723>. The `else` branch must remove
the `TypedDict`; otherwise iterating `result` introduces its string keys into the element type:

```py
import collections
from typing import TypedDict

class Result(TypedDict):
    name: str
    score: float

class MyDict(collections.defaultdict[str, float]):
    def add(self, text: str, result: Result | list[Result]) -> None:
        if isinstance(result, dict):
            self[result["name"]] += result["score"] * len(text)
        else:
            for item in result:
                self.add(text, item)
```

The negative branch removes a `TypedDict` member, making attributes from the remaining class
available:

```py
class Package:
    ecosystem: str

class Vulnerability:
    package: Package
    patched_versions: str | None

def narrow_typeddict_or_class(value: A | Vulnerability) -> None:
    if isinstance(value, dict):
        pass
    else:
        reveal_type(value.package.ecosystem)  # revealed: str
```

## Narrowing with named expressions (walrus operator)

When `isinstance()` is used with a named expression, the target of the named expression should be
narrowed.

```py
def get_value() -> int | str:
    return 1

def f():
    if isinstance(x := get_value(), int):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: str
```
