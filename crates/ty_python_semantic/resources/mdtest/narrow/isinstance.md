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
        reveal_type(y)  # revealed: (...) -> Unknown
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

## `NewType` instances and concrete-base subclasses

A `NewType` constructor returns its argument unchanged at runtime, and runtime class checks ignore
its static tag. The resulting value can therefore still be an instance of a subclass of its concrete
base. For example, `UserId(True)` is valid because `bool` is a subtype of `int`, and the returned
value remains a `bool`.

```py
from typing import NewType

class Base: ...
class Child(Base): ...

BrandedBase = NewType("BrandedBase", Base)
UserId = NewType("UserId", int)

UserId(True)

def narrow_branded_subclass(value: BrandedBase) -> None:
    if isinstance(value, Child):
        reveal_type(value)  # revealed: BrandedBase & Child
    else:
        reveal_type(value)  # revealed: BrandedBase & ~Child

def narrow_branded_boolean(value: UserId) -> None:
    if isinstance(value, bool):
        reveal_type(value)  # revealed: UserId & bool
    else:
        reveal_type(value)  # revealed: UserId & ~bool
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

### Strict mode

```toml
[environment]
python-version = "3.12"

[analysis]
strict-generic-narrowing = true
```

In strict mode, narrowing to a generic class using `isinstance()` uses the top materialization of
the generic. With a covariant generic, this is equivalent to using the upper bound of the type
parameter (by default, `object`):

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

A bounded covariant generic uses its declared upper bound rather than `object`:

```py
class BoundedCovariant[T: int]:
    def get(self) -> T:
        raise NotImplementedError

def _(x: object):
    if isinstance(x, BoundedCovariant):
        reveal_type(x)  # revealed: BoundedCovariant[int]
        reveal_type(x.get())  # revealed: int
```

Negative narrowing must exclude every specialization of a bounded generic, including a gradual one.

```py
from typing import Any

def excludes_bounded_generic(value: BoundedCovariant[Any] | bool) -> bool:
    if isinstance(value, BoundedCovariant):
        reveal_type(value)  # revealed: BoundedCovariant[int & Any]
        return False

    reveal_type(value)  # revealed: bool
    return value
```

The same exclusion applies when the generic appears in a tuple of runtime classes.

```py
def excludes_bounded_generic_tuple(
    value: BoundedCovariant[Any] | bool | bytes,
) -> bool:
    if isinstance(value, (BoundedCovariant, bytes)):
        reveal_type(value)  # revealed: BoundedCovariant[int & Any] | bytes
        return False

    reveal_type(value)  # revealed: bool
    return value
```

Constrained type parameters preserve the materialization of the generic class while making the union
of valid constraints available when reading a covariant attribute:

```py
class ConstrainedCovariant[T: (int, str)]:
    def get(self) -> T:
        raise NotImplementedError

def _(x: object):
    if isinstance(x, ConstrainedCovariant):
        reveal_type(x)  # revealed: Top[ConstrainedCovariant[Unknown]]
        reveal_type(x.get())  # revealed: int | str
```

Constrained generics must also be excluded by negative narrowing.

```py
def excludes_constrained_generic(value: ConstrainedCovariant[Any] | bool) -> bool:
    if isinstance(value, ConstrainedCovariant):
        reveal_type(value)  # revealed: ConstrainedCovariant[Any]
        return False

    reveal_type(value)  # revealed: bool
    return value
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
        reveal_type(x.a)  # revealed: int
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

The built-in `tuple` stores its variable-length shape separately from its generic type argument.
Narrowing must preserve and materialize that shape.

```py
def narrow_tuple(value: object) -> None:
    if isinstance(value, tuple):
        reveal_type(value)  # revealed: tuple[object, ...]
```

A tuple subclass retains its nominal type and inherits its tuple shape from its specialized base.
The subclass's own type parameter is still materialized using its declared bound.

```py
class BoundedTuple[T: int](tuple[T, str]): ...

def narrow_tuple_subclass(value: object) -> None:
    if isinstance(value, BoundedTuple):
        reveal_type(value)  # revealed: BoundedTuple[int]
        reveal_type(value[0])  # revealed: int
        reveal_type(value[1])  # revealed: str
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

Negative `issubclass()` narrowing also excludes every specialization of a bounded generic.

```py
def excludes_bounded_generic_subclass(
    cls: type[BoundedCovariant[Any]] | type[bool],
) -> type[bool]:
    if issubclass(cls, BoundedCovariant):
        reveal_type(cls)  # revealed: type[BoundedCovariant[Any]]
        return bool

    reveal_type(cls)  # revealed: <class 'bool'>
    return cls
```

### Gradual mode

```toml
[environment]
python-version = "3.12"

[analysis]
strict-generic-narrowing = false
```

In gradual mode, narrowing to a generic class using `isinstance()` preserves any compatible
specialization from the original type. If the original type does not provide a specialization, we
intersect with the `Unknown` specialization. The negative branch still excludes the top
materialization because a failed `isinstance()` check rules out every specialization of the class.

```py
class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

def _(x: object):
    if isinstance(x, Covariant):
        # `object & Covariant[Unknown]` simplifies to `Covariant[Unknown]`.
        reveal_type(x)  # revealed: Covariant[Unknown]
        reveal_type(x.get())  # revealed: Unknown
    else:
        reveal_type(x)  # revealed: ~Covariant[object]
```

For contravariant generics, we similarly intersect with the `Unknown` specialization:

```py
class Contravariant[T]:
    def push(self, x: T) -> None: ...

def _(x: object):
    if isinstance(x, Contravariant):
        reveal_type(x)  # revealed: Contravariant[Unknown]
        x.push(42)
        x.push("foo")
    else:
        reveal_type(x)  # revealed: ~Contravariant[Never]
```

Similarly, for invariant generics we intersect with the `Unknown` specialization. Reading produces
`Unknown`, while writing accepts arguments of any type:

```py
class Invariant[T]:
    def push(self, x: T) -> None: ...
    def get(self) -> T:
        raise NotImplementedError

def _(x: object):
    if isinstance(x, Invariant):
        reveal_type(x)  # revealed: Invariant[Unknown]
        reveal_type(x.get)  # revealed: bound method Invariant[Unknown].get() -> Unknown
        reveal_type(x.get())  # revealed: Unknown
        reveal_type(x.push)  # revealed: bound method Invariant[Unknown].push(x: Unknown) -> None
        x.push(42)
        x.push("foo")
    else:
        reveal_type(x)  # revealed: ~Top[Invariant[Unknown]]
```

Narrowing already specialized generics preserves their concrete type arguments:

```py
class P: ...

def _(x: Covariant[P], y: Contravariant[P], z: Invariant[P]):
    if isinstance(x, Covariant):
        reveal_type(x)  # revealed: Covariant[P]
    if isinstance(y, Contravariant):
        reveal_type(y)  # revealed: Contravariant[P]
    if isinstance(z, Invariant):
        reveal_type(z)  # revealed: Invariant[P]
```

Specialized base classes also determine the type arguments of matching subclasses, including
subclasses with a stricter variance:

```py
class SubOfCovariant[T](Covariant[T]): ...
class SubOfContravariant[T](Contravariant[T]): ...
class SubOfInvariant[T](Invariant[T]): ...

class InvariantSubOfCovariant[T](Covariant[T]):
    def push(self, value: T) -> None: ...

class InvariantSubOfContravariant[T](Contravariant[T]):
    def get(self) -> T:
        raise NotImplementedError

def narrow_generic_subclasses(covariant: Covariant[P], contravariant: Contravariant[P], invariant: Invariant[P]) -> None:
    if isinstance(covariant, SubOfCovariant):
        reveal_type(covariant)  # revealed: SubOfCovariant[P]

    if isinstance(contravariant, SubOfContravariant):
        reveal_type(contravariant)  # revealed: SubOfContravariant[P]

    if isinstance(invariant, SubOfInvariant):
        reveal_type(invariant)  # revealed: SubOfInvariant[P]

    if isinstance(covariant, InvariantSubOfCovariant):
        reveal_type(covariant)  # revealed: InvariantSubOfCovariant[P]

    if isinstance(contravariant, InvariantSubOfContravariant):
        reveal_type(contravariant)  # revealed: InvariantSubOfContravariant[P]
```

Narrowing unions and intersections preserves unrelated types when they can overlap with the checked
class, while excluding unrelated final classes:

```py
from typing import Sequence, final
from ty_extensions import Intersection

@final
class Item: ...

class OpenItem: ...

def _(value: Item | OpenItem | Sequence[int]) -> None:
    if isinstance(value, list):
        reveal_type(value)  # revealed: (OpenItem & list[Unknown]) | list[int]

def _(
    value: Intersection[OpenItem, Sequence[int]],
) -> None:
    if isinstance(value, list):
        reveal_type(value)  # revealed: OpenItem & list[int]
```

When an intersection contains multiple specialized bases, each base contributes its known type
arguments to a matching subclass:

```py
class Left[L]: ...
class Right[R]: ...

class Both[L, R](Left[L], Right[R]):
    left: L
    right: R

def _(value: Intersection[Left[int], Right[str]]) -> None:
    if isinstance(value, Both):
        reveal_type(value)  # revealed: Both[int, str]
        reveal_type(value.left)  # revealed: int
        reveal_type(value.right)  # revealed: str
```

Subclass type arguments are inferred through their actual inheritance relationship, so this also
works correctly if type parameters change position:

```py
class Base[A, B]: ...
class Child[X, Y](Base[Y, X]): ...

def _(value: Base[int, str]) -> None:
    if isinstance(value, Child):
        reveal_type(value)  # revealed: Child[str, int]
```

A subclass type parameter that cannot be inferred from its base remains `Unknown`:

```py
class PartiallyInferredChild[Extra1, T, Extra2](Sequence[T]): ...

def _(value: Sequence[int]) -> None:
    if isinstance(value, PartiallyInferredChild):
        reveal_type(value)  # revealed: PartiallyInferredChild[Unknown, int, Unknown]
```

If we're "narrowing" in the opposite direction, we retain the existing subclass specialization:

```py
def _(covariant: SubOfCovariant[P], contravariant: SubOfContravariant[P], invariant: SubOfInvariant[P]) -> None:
    if isinstance(covariant, Covariant):
        reveal_type(covariant)  # revealed: SubOfCovariant[P]

    if isinstance(contravariant, Contravariant):
        reveal_type(contravariant)  # revealed: SubOfContravariant[P]

    if isinstance(invariant, Invariant):
        reveal_type(invariant)  # revealed: SubOfInvariant[P]
```

This also works for runtime-checkable protocols:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class Reader[T](Protocol):
    def read(self) -> T: ...

class Concrete[T]:
    def read(self) -> T:
        raise NotImplementedError

def _(value: Concrete[int]) -> None:
    if isinstance(value, Reader):
        reveal_type(value)  # revealed: Concrete[int]
        reveal_type(value.read())  # revealed: int
```

## Use cases: `isinstance` narrowing and generics

### Strict mode

```toml
[environment]
python-version = "3.12"

[analysis]
strict-generic-narrowing = true
```

#### Covariance

Narrowing from `object` via `isinstance(.., Sequence)`:

```py
from typing import Sequence, final

def _(xs: object):
    if isinstance(xs, Sequence):
        reveal_type(xs)  # revealed: Sequence[object]
        for x in xs:
            reveal_type(x)  # revealed: object
    else:
        reveal_type(xs)  # revealed: ~Sequence[object]
```

Narrowing from `Item | Sequence[Item]` via `isinstance(.., Sequence)`:

```py
@final
class Item: ...

def _(xs: Item | Sequence[Item]):
    if isinstance(xs, Sequence):
        reveal_type(xs)  # revealed: Sequence[Item]
        for x in xs:
            reveal_type(x)  # revealed: Item
    else:
        reveal_type(xs)  # revealed: Item
```

Narrowing from (non-final) `OpenItem | Sequence[OpenItem]` via `isinstance(.., Sequence)`:

```py
class OpenItem: ...

def _(xs: OpenItem | Sequence[OpenItem]):
    if isinstance(xs, Sequence):
        reveal_type(xs)  # revealed: (OpenItem & Sequence[object]) | Sequence[OpenItem]
        for x in xs:
            reveal_type(x)  # revealed: object
    else:
        reveal_type(xs)  # revealed: OpenItem & ~Sequence[object]
```

#### Invariance

Narrowing from `object` via `isinstance(.., list)`:

```py
def _(xs: object):
    if isinstance(xs, list):
        reveal_type(xs)  # revealed: Top[list[Unknown]]
        for x in xs:
            reveal_type(x)  # revealed: object

        # This is an error in strict mode:
        # error: [invalid-argument-type] "Expected `Never`, found `Literal[1]`"
        xs.append(1)

    else:
        reveal_type(xs)  # revealed: ~Top[list[Unknown]]
```

Narrowing from `Item | list[Item]` via `isinstance(.., list)`:

```py
from typing import final

@final
class Item: ...

def _(xs: Item | list[Item]):
    if isinstance(xs, list):
        reveal_type(xs)  # revealed: list[Item]
        for x in xs:
            reveal_type(x)  # revealed: Item
    else:
        reveal_type(xs)  # revealed: Item
```

Narrowing from (non-final) `OpenItem | list[OpenItem]` via `isinstance(.., list)`:

```py
class OpenItem: ...

def _(xs: OpenItem | list[OpenItem]):
    if isinstance(xs, list):
        reveal_type(xs)  # revealed: (OpenItem & Top[list[Unknown]]) | list[OpenItem]
        for x in xs:
            reveal_type(x)  # revealed: object
    else:
        reveal_type(xs)  # revealed: OpenItem & ~Top[list[Unknown]]
```

#### Exhaustiveness checking

```py
def _(xs: list[str] | set[str]) -> str:
    if isinstance(xs, list):
        return "it's a list!"
    elif isinstance(xs, set):
        return "it's a set!"
```

#### Invariance with bounded type variables

A value of a type variable bounded by `str` can also be an instance of a `Box` specialization
through multiple inheritance. Checking `isinstance(value, Box)` cannot establish that this
specialization is `Box[T]`, so the intersection with `T` survives and the return is rejected.

```py
class Box[T]:
    value: T

def narrow_box[T: str](value: Box[T] | T) -> Box[T]:
    if isinstance(value, Box):
        reveal_type(value)  # revealed: Box[T@narrow_box] | (T@narrow_box & Top[Box[Unknown]])
        return value  # error: [invalid-return-type]

    reveal_type(value)  # revealed: T@narrow_box & ~Top[Box[Unknown]]
    raise TypeError
```

### Gradual mode

```toml
[environment]
python-version = "3.12"

[analysis]
strict-generic-narrowing = false
```

#### Covariance

Narrowing from `object` via `isinstance(.., Sequence)`:

```py
from typing import Sequence, final

def _(xs: object):
    if isinstance(xs, Sequence):
        reveal_type(xs)  # revealed: Sequence[Unknown]
        for x in xs:
            reveal_type(x)  # revealed: Unknown
    else:
        reveal_type(xs)  # revealed: ~Sequence[object]
```

Narrowing from `Item | Sequence[Item]` via `isinstance(.., Sequence)`:

```py
@final
class Item: ...

def _(xs: Item | Sequence[Item]):
    if isinstance(xs, Sequence):
        reveal_type(xs)  # revealed: Sequence[Item]
        for x in xs:
            reveal_type(x)  # revealed: Item
    else:
        reveal_type(xs)  # revealed: Item
```

Narrowing from (non-final) `OpenItem | Sequence[OpenItem]` via `isinstance(.., Sequence)`:

```py
class OpenItem: ...

def _(xs: OpenItem | Sequence[OpenItem]):
    if isinstance(xs, Sequence):
        reveal_type(xs)  # revealed: (OpenItem & Sequence[Unknown]) | Sequence[OpenItem]
        for x in xs:
            reveal_type(x)  # revealed: Unknown | OpenItem
    else:
        reveal_type(xs)  # revealed: OpenItem & ~Sequence[object]
```

#### Invariance

Narrowing from `object` via `isinstance(.., list)`:

```py
def _(xs: object):
    if isinstance(xs, list):
        reveal_type(xs)  # revealed: list[Unknown]
        for x in xs:
            reveal_type(x)  # revealed: Unknown

        xs.append(1)
        xs.append("foo")

    else:
        reveal_type(xs)  # revealed: ~Top[list[Unknown]]
```

Narrowing from `Item | list[Item]` via `isinstance(.., list)`:

```py
from typing import final

@final
class Item: ...

def _(xs: Item | list[Item]):
    if isinstance(xs, list):
        reveal_type(xs)  # revealed: list[Item]
        for x in xs:
            reveal_type(x)  # revealed: Item
    else:
        reveal_type(xs)  # revealed: Item
```

Narrowing from (non-final) `OpenItem | list[OpenItem]` via `isinstance(.., list)`:

```py
class OpenItem: ...

def _(xs: OpenItem | list[OpenItem]):
    if isinstance(xs, list):
        reveal_type(xs)  # revealed: (OpenItem & list[Unknown]) | list[OpenItem]
        for x in xs:
            reveal_type(x)  # revealed: Unknown | OpenItem
    else:
        reveal_type(xs)  # revealed: OpenItem & ~Top[list[Unknown]]
```

#### Exhaustiveness checking

```py
def _(xs: list[str] | set[str]) -> str:
    if isinstance(xs, list):
        return "it's a list!"
    elif isinstance(xs, set):
        return "it's a set!"
```

#### Invariance with bounded type variables

A value of a type variable bounded by `str` can also be an instance of a `Box` specialization
through multiple inheritance. In gradual mode, `isinstance(value, Box)` preserves this overlap using
`Box[Unknown]`, which is assignable to `Box[T]`, so the return statement is (unsoundly) accepted.

```py
class Box[T]:
    value: T

def narrow_box[T: str](value: Box[T] | T) -> Box[T]:
    if isinstance(value, Box):
        reveal_type(value)  # revealed: Box[T@narrow_box] | (T@narrow_box & Box[Unknown])
        return value

    reveal_type(value)  # revealed: T@narrow_box & ~Top[Box[Unknown]]
    raise TypeError
```

## Narrowing recursively bounded generics (strict mode)

An `isinstance()` check must not recurse indefinitely when a generic bound refers to its own class.

```toml
[environment]
python-version = "3.12"

[analysis]
strict-generic-narrowing = true
```

```py
from typing import Any

class Recursive[T: "Recursive[Any]"]: ...

def narrow(value: object) -> None:
    if isinstance(value, Recursive):
        reveal_type(value)  # revealed: Recursive[object]
```

A self-referential bound must also be safe when its recursion is hidden behind a type alias.

```py
class AliasedRecursive[T: "RecursiveAlias"]: ...

type RecursiveAlias = AliasedRecursive[Any]

def narrow_alias(value: object) -> None:
    if isinstance(value, AliasedRecursive):
        reveal_type(value)  # revealed: AliasedRecursive[object]
```

The same cycle recovery must handle bounds shared by mutually recursive generic classes.

```py
class Left[T: "Right[Any]"]: ...
class Right[U: Left[Any]]: ...

def narrow_mutual(value: object) -> None:
    if isinstance(value, Left):
        reveal_type(value)  # revealed: Left[object]

    if isinstance(value, Right):
        reveal_type(value)  # revealed: Right[object]
```

## Narrowing recursively bounded generics (gradual mode)

An `isinstance()` check must not recurse indefinitely when a generic bound refers to its own class.

```toml
[environment]
python-version = "3.12"

[analysis]
strict-generic-narrowing = false
```

```py
from typing import Any

class Recursive[T: "Recursive[Any]"]: ...

def narrow(value: object) -> None:
    if isinstance(value, Recursive):
        reveal_type(value)  # revealed: Recursive[Unknown]
```

A self-referential bound must also be safe when its recursion is hidden behind a type alias.

```py
class AliasedRecursive[T: "RecursiveAlias"]: ...

type RecursiveAlias = AliasedRecursive[Any]

def narrow_alias(value: object) -> None:
    if isinstance(value, AliasedRecursive):
        reveal_type(value)  # revealed: AliasedRecursive[Unknown]
```

The same cycle recovery must handle bounds shared by mutually recursive generic classes.

```py
class Left[T: "Right[Any]"]: ...
class Right[U: Left[Any]]: ...

def narrow_mutual(value: object) -> None:
    if isinstance(value, Left):
        reveal_type(value)  # revealed: Left[Unknown]

    if isinstance(value, Right):
        reveal_type(value)  # revealed: Right[Unknown]
```

## Narrowing generic defaults in Python 3.13

When a type parameter has a bare `Any` default, narrowing still materializes the substituted
typevar. The default isn't used during `isinstance` narrowing (the type parameter gets `Unknown`
instead), so the default value is irrelevant here:

```toml
[environment]
python-version = "3.13"

[analysis]
strict-generic-narrowing = true
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

`isinstance(value, Box)` checks the runtime class, not the type argument used to specialize it.
Narrowing must therefore preserve the original type argument instead of substituting `Box`'s
default.

```py
from typing import assert_never, final

@final
class Box[T: str = str]:
    value: T

    def __init__(self, value: T) -> None: ...

def box_with_default[T: str = str](value: Box[T] | T) -> Box[T]:
    if isinstance(value, Box):
        reveal_type(value)  # revealed: Box[T@box_with_default]
        return value

    if not isinstance(value, Box):
        reveal_type(value)  # revealed: T@box_with_default
        return Box[T](value)

    assert_never(value)
```

When `isinstance()` narrows a value of type `object` to a tuple subclass, its type argument comes
from the declared upper bound, not the default. Its element types are inherited from the specialized
base.

```py
class DefaultedTuple[T: int = bool](tuple[T, str]): ...

def narrow_defaulted_tuple(value: object) -> None:
    if isinstance(value, DefaultedTuple):
        reveal_type(value)  # revealed: DefaultedTuple[int]
        reveal_type(value[0])  # revealed: int
        reveal_type(value[1])  # revealed: str
```

Negative narrowing also excludes gradual specializations of the defaulted tuple subclass.

```py
def excludes_defaulted_tuple(value: DefaultedTuple[Any] | bool) -> bool:
    if isinstance(value, DefaultedTuple):
        reveal_type(value)  # revealed: DefaultedTuple[int & Any]
        reveal_type(value[0])  # revealed: int & Any
        reveal_type(value[1])  # revealed: str
        return False

    reveal_type(value)  # revealed: bool
    return value
```

## Narrowing bounded generic defaults in gradual mode

In gradual mode, narrowing a value of type `object` to a tuple subclass leaves its type argument
`Unknown`.

```toml
[environment]
python-version = "3.13"

[analysis]
strict-generic-narrowing = false
```

```py
class DefaultedTuple[T: int = bool](tuple[T, str]): ...

def narrow_defaulted_tuple(value: object) -> None:
    if isinstance(value, DefaultedTuple):
        reveal_type(value)  # revealed: DefaultedTuple[Unknown]
        reveal_type(value[0])  # revealed: Unknown
        reveal_type(value[1])  # revealed: str
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

Narrowing unions of `int` and multiple TypedDicts using `isinstance(x, dict)` should not panic
during type ordering of normalized intersection types. Regression test for
<https://github.com/astral-sh/ty/issues/2451>.

```py
from typing import Any, TypedDict, cast

class A(TypedDict):
    x: str

class B(TypedDict):
    y: str

T = int | A | B

def test(a: Any, items: list[T]) -> None:
    combined = a or items
    v = combined[0]
    if isinstance(v, dict):
        cast(T, v)  # no panic
```

## Narrowing with named expressions (walrus operator)

When `isinstance()` is used with a named expression, the target of the named expression should be
narrowed. When the `isinstance()` check is the value of a named expression, its argument should also
be narrowed.

```py
def get_value() -> int | str:
    return 1

def f():
    if isinstance(x := get_value(), int):
        reveal_type(x)  # revealed: int
    else:
        reveal_type(x)  # revealed: str

    value = get_value()
    if result := isinstance(value, int):
        reveal_type(value)  # revealed: int
        reveal_type(result)  # revealed: Literal[True]
    else:
        reveal_type(value)  # revealed: str
        reveal_type(result)  # revealed: Literal[False]
```

## Preserving TypedDict interfaces when narrowing mappings

A `TypedDict` is always a dictionary at runtime, but its static interface deliberately disallows
operations that could remove required keys or introduce undeclared ones. Narrowing to `dict`,
`Mapping`, or `MutableMapping` must not discard these restrictions.

Use a `TypedDict` with one required key and one optional key to distinguish safe operations from
those that could invalidate its declared shape.

```py
from typing import TypedDict, Mapping, MutableMapping
from typing_extensions import NotRequired

class Payload(TypedDict):
    key: int
    optional: NotRequired[str]
```

Narrowing directly to `dict` preserves both the required-key restrictions and the optional key's
known type.

```py
def narrow_typed_dict_to_dict(value: int | Payload) -> None:
    if isinstance(value, dict):
        reveal_type(value)  # revealed: Payload
        reveal_type(value["key"])  # revealed: int
        value["key"] = 1
        value["optional"] = "present"
        reveal_type(value.pop("optional"))  # revealed: str

        # error: [unresolved-attribute]
        value.clear()
        # error: [invalid-argument-type] "Cannot pop required field 'key' from TypedDict `Payload`"
        value.pop("key")
        # error: [invalid-key] "Unknown key "unexpected" for TypedDict `Payload`"
        value["unexpected"] = 1
        # error: [invalid-argument-type] "Cannot delete required key "key" from TypedDict `Payload`"
        del value["key"]
```

Same for `MutableMapping`:

```py
def narrow_typed_dict_to_mutable_mapping(value: Payload) -> None:
    if isinstance(value, MutableMapping):
        reveal_type(value)  # revealed: Payload
        # error: [unresolved-attribute]
        value.clear()
```

And for `Mapping`:

```py
def narrow_typed_dict_to_mapping(value: Payload) -> None:
    if isinstance(value, Mapping):
        reveal_type(value)  # revealed: Payload
        # error: [unresolved-attribute]
        value.clear()
```

A type alias must retain the same `TypedDict` interface.

```py
PayloadAlias = Payload

def narrow_aliased_typed_dict_to_dict(value: PayloadAlias) -> None:
    if isinstance(value, dict):
        reveal_type(value)  # revealed: Payload
        # error: [unresolved-attribute]
        value.clear()
```
