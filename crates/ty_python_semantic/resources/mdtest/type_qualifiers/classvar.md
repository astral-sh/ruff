# `typing.ClassVar`

[`typing.ClassVar`] is a type qualifier that is used to indicate that a class variable may not be
written to from instances of that class.

This test makes sure that we discover the type qualifier while inferring types from an annotation.
For more details on the semantics of pure class variables, see [this test](../attributes.md).

## Basic

```py
import typing
from typing import ClassVar, Annotated

class C:
    a: ClassVar[int] = 1
    b: Annotated[ClassVar[int], "the annotation for b"] = 1
    c: ClassVar[Annotated[int, "the annotation for c"]] = 1
    d: ClassVar = 1
    e: "ClassVar[int]" = 1
    f: typing.ClassVar = 1

reveal_type(C.a)  # revealed: int
reveal_type(C.b)  # revealed: int
reveal_type(C.c)  # revealed: int
reveal_type(C.d)  # revealed: Unknown | Literal[1]
reveal_type(C.e)  # revealed: int
reveal_type(C.f)  # revealed: Unknown | Literal[1]

c = C()

# error: [invalid-attribute-access]
c.a = 2
# error: [invalid-attribute-access]
c.b = 2
# error: [invalid-attribute-access]
c.c = 2
# error: [invalid-attribute-access]
c.d = 2
# error: [invalid-attribute-access]
c.e = 2
# error: [invalid-attribute-access]
c.f = 3
```

## From stubs

This is a regression test for a bug where we did not properly keep track of type qualifiers when
accessed from stub files.

`module.pyi`:

```pyi
from typing import ClassVar

class C:
    a: ClassVar[int]
```

`main.py`:

```py
from module import C

c = C()
c.a = 2  # error: [invalid-attribute-access]
```

## Conflicting type qualifiers

We currently ignore conflicting qualifiers and simply union them, which is more conservative than
intersecting them. This means that we consider `a` to be a `ClassVar` here:

```py
from typing import ClassVar

def flag() -> bool:
    return True

class C:
    if flag():
        a: ClassVar[int] = 1
    else:
        a: str

reveal_type(C.a)  # revealed: int | str

c = C()

# error: [invalid-attribute-access]
c.a = 2
```

## Too many arguments

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` expected exactly 1 argument, got 2"
    x: ClassVar[int, str] = 1
```

## Trailing comma creates a tuple

A trailing comma in a subscript creates a single-element tuple. We need to handle this gracefully
and emit a proper error rather than crashing (see
[ty#1793](https://github.com/astral-sh/ty/issues/1793)).

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Tuple literals are not allowed in this context in a type expression: Did you mean `tuple[()]`?"
    x: ClassVar[(),]

# error: [invalid-attribute-access] "Cannot assign to ClassVar `x` from an instance of type `C`"
C().x = 42
reveal_type(C.x)  # revealed: Unknown
```

This also applies when the trailing comma is inside the brackets (see
[ty#1768](https://github.com/astral-sh/ty/issues/1768)):

```py
from typing import ClassVar

class D:
    # A trailing comma here doesn't change the meaning; it's still one argument.
    a: ClassVar[int,] = 1

reveal_type(D.a)  # revealed: int
```

## `ClassVar` cannot contain non-self type variables

`ClassVar` cannot include type variables at any level of nesting.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, TypeVar, ParamSpec, Generic

T = TypeVar("T")
P = ParamSpec("P")

class C(Generic[T, P]):
    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    a: ClassVar[T]

    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    b: ClassVar[list[T]]

    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    c: ClassVar[int | T]

    # error: [invalid-type-form] "Bare ParamSpec `P` is not valid in this context"
    d: ClassVar[P]

    # No error: no type variables
    e: ClassVar[int] = 1

# PEP 695 syntax
class D[T]:
    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    x: ClassVar[T]

    # error: [invalid-type-form] "`ClassVar` cannot contain type variables"
    y: ClassVar[dict[str, T]]
```

## Type variables inside aliases

`ClassVar` rejects type variables in an alias's value, whether they appear directly or inside
another type.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Generic, TypeVar

type DirectAlias[T] = T
type Items[T] = list[T]

class Holder[T]:
    # error: [invalid-type-form]
    direct: ClassVar[DirectAlias[T]]
    # error: [invalid-type-form]
    nested: ClassVar[Items[T]]
```

The same restriction applies to legacy type variables.

```py
T = TypeVar("T")

class LegacyHolder(Generic[T]):
    # error: [invalid-type-form]
    nested: ClassVar[Items[T]]
```

Arguments that do not appear in the alias's value are allowed. This includes `T` in `object | T`,
which simplifies to `object`.

```py
type Ignored[T] = int
type Either[T, U] = T | U

class Unused[T]:
    ignored: ClassVar[Ignored[T]]  # no diagnostic
    simplified: ClassVar[Either[object, T]]  # no diagnostic

class LegacyUnused(Generic[T]):
    ignored: ClassVar[Ignored[T]]  # no diagnostic
    simplified: ClassVar[Either[object, T]]  # no diagnostic
```

## Type variables inside recursive aliases

Recursive aliases can expose an argument only after passing it to a different parameter. Parameters
that remain unused throughout the recursion are allowed.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar

type Shift[A, B] = A | list[Shift[B, int]]
type RecursiveIgnored[A, B] = B | list[RecursiveIgnored[A, int]]

class RecursiveHolder[T]:
    # error: [invalid-type-form]
    exposed: ClassVar[Shift[int, T]]
    ignored: ClassVar[RecursiveIgnored[T, int]]  # no diagnostic
    simplified: ClassVar[Shift[object, T]]  # no diagnostic
```

## Recursive aliases with changing type arguments

An unused argument remains valid when each recursive reference wraps it in another `list`. It can be
unused because it appears only in recursive references, or because a union simplifies to `object`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar

type Absorbed[A, B] = tuple[A | B, Absorbed[A, list[B]]]
type Recursive[T] = list[Recursive[list[T]]]

class Holder[T]:
    simplified: ClassVar[Absorbed[object, T]]  # no diagnostic
    recursive: ClassVar[Recursive[T]]  # no diagnostic
```

We do not yet reject `Shift[int, T]`, even though `T` becomes a tuple element after the first
recursive reference:

```py
type Shift[A, B] = tuple[A, Shift[B, list[A]]]

class Invalid[T]:
    # TODO: Reject T when it becomes the first argument of Shift.
    exposed: ClassVar[Shift[int, T]]
```

A separate reference to `Shift[T, list[int]]` exposes `T` directly, even when it also occurs inside
an earlier recursive reference.

```py
class AlsoInvalid[T]:
    # error: [invalid-type-form]
    exposed: ClassVar[tuple[Shift[int, T], Shift[T, list[int]]]]
```

## `ClassVar` can contain `Self`

`Self` is allowed inside `ClassVar`.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import ClassVar, Self

class Base:
    all_instances: ClassVar[list[Self]]

    def method(self):
        reveal_type(self.all_instances)  # revealed: list[Self@method]

    @classmethod
    def cls_method(cls):
        reveal_type(cls.all_instances)  # revealed: list[Self@cls_method]

reveal_type(Base.all_instances)  # revealed: list[Base]

class Sub(Base): ...

reveal_type(Sub.all_instances)  # revealed: list[Sub]
```

The type parameters in `Self`'s bound do not make `Self` invalid in a generic class.

```py
from typing import Generic, TypeVar

U = TypeVar("U")

class GenericBase(Generic[U]):
    direct: ClassVar[Self]  # no diagnostic
    nested: ClassVar[list[Self]]  # no diagnostic
```

Assignments through class objects should bind `Self` when writing a `ClassVar`, matching read-side
behavior. This remains permissive for `type[Base]` values even though `ClassVar[Self]` in non-final
classes is unsound.

```py
from typing import ClassVar, Self, TypeVar

class Saved:
    latest: ClassVar[Self]

    def save(self) -> None:
        type(self).latest = self

Saved.latest = Saved()

reveal_type(Saved.latest)  # revealed: Saved

class SavedSub(Saved): ...

reveal_type(SavedSub.latest)  # revealed: SavedSub

SavedSub.latest = SavedSub()

SavedSub.latest = Saved()  # error: [invalid-assignment]

def store_saved(cls: type[Saved]) -> None:
    cls.latest = Saved()

T = TypeVar("T", bound=Saved)

def store_generic(cls: type[T], value: T) -> None:
    cls.latest = value
```

Assignments through gradual class objects remain permissive.

```py
from typing import Any, ClassVar, reveal_type

class DynamicSaved:
    count: ClassVar[int]

def store_any(cls: type[Any], value: Any) -> None:
    cls.count = value
    reveal_type(cls.count)  # revealed: Any
```

## `Self` in PEP 695 generic classes

`Self` is also valid in generic classes declared with PEP 695 syntax.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Self

class GenericBase[U]:
    direct: ClassVar[Self]  # no diagnostic
    nested: ClassVar[list[Self]]  # no diagnostic
```

## Generic callable signatures

Type variables bound by a callable's own signature are allowed in `ClassVar`. `CallableTypeOf`
preserves a function's generic signature without specializing it.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, TypeVar
from ty_extensions._internal import CallableTypeOf, TypeOf

def identity[T](value: T) -> T:
    return value

class Holder:
    callback: ClassVar[CallableTypeOf[identity]]  # no diagnostic
    function: ClassVar[TypeOf[identity]]  # no diagnostic
```

The same applies to a function that uses a legacy type variable.

```py
U = TypeVar("U")

def legacy_identity(value: U) -> U:
    return value

class LegacyHolder:
    callback: ClassVar[CallableTypeOf[legacy_identity]]  # no diagnostic
```

## Captured type variables in callables

A callable can still capture a type variable from an enclosing scope. Its own type parameters do not
bind that captured variable.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar
from ty_extensions._internal import CallableTypeOf, TypeOf

def outer[T]():
    def callback[U](value: T, other: U) -> U:
        return other

    class Holder:
        # error: [invalid-type-form]
        value: ClassVar[CallableTypeOf[callback]]
        # error: [invalid-type-form]
        function: ClassVar[TypeOf[callback]]
```

## Protocols with generic methods

A protocol method binds its own type parameters, so the protocol is valid in a `ClassVar`
annotation.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Protocol

class Callback(Protocol):
    def __call__[T](self, value: T) -> T: ...

class Holder:
    callback: ClassVar[Callback]  # no diagnostic
```

This remains valid when the protocol is local to a function.

```py
def outer():
    class Callback(Protocol):
        def __call__[T](self, value: T) -> T: ...

    class Holder:
        callback: ClassVar[Callback]  # no diagnostic
```

## Generic property accessors

A property's getter and setter bind their own type parameters, just like other methods. Those
parameters do not make the protocol invalid in `ClassVar`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Protocol, TypeVar

class GenericProperties(Protocol):
    @property
    def getter[T](self) -> tuple[T, T]: ...
    @property
    def setter(self) -> object: ...
    @setter.setter
    def setter[T](self, value: tuple[T, T]) -> None: ...

class Holder:
    value: ClassVar[GenericProperties]  # no diagnostic
```

The same applies to accessors that use legacy type variables.

```py
T = TypeVar("T")

class LegacyProperties(Protocol):
    @property
    def getter(self) -> tuple[T, T]: ...
    @property
    def setter(self) -> object: ...
    @setter.setter
    def setter(self, value: tuple[T, T]) -> None: ...

class LegacyHolder:
    value: ClassVar[LegacyProperties]  # no diagnostic
```

## Captured type variables in property accessors

An accessor's own type parameters do not bind variables captured from an enclosing function.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Protocol, TypeVar

def outer[T]():
    class Getter(Protocol):
        @property
        def value[U](self) -> tuple[U, T]: ...

    class Setter(Protocol):
        @property
        def value(self) -> object: ...
        @value.setter
        def value[U](self, value: tuple[U, T]) -> None: ...

    class Holder:
        # error: [invalid-type-form]
        getter: ClassVar[Getter]
        # error: [invalid-type-form]
        setter: ClassVar[Setter]
```

Legacy accessors also retain the distinction between their own type variables and captured
variables.

```py
T = TypeVar("T")
U = TypeVar("U")

def legacy_outer(value: T):
    class Getter(Protocol):
        @property
        def value(self) -> tuple[U, T]: ...

    class Setter(Protocol):
        @property
        def value(self) -> object: ...
        @value.setter
        def value(self, value: tuple[U, T]) -> None: ...

    class Holder:
        # error: [invalid-type-form]
        getter: ClassVar[Getter]
        # error: [invalid-type-form]
        setter: ClassVar[Setter]
```

## Extra parameters in property accessors

Only the getter's return type and the setter's value type are exposed by a property. Captures in
optional extra parameters do not affect its use in `ClassVar`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Protocol

def extra_parameters[T]():
    class Property(Protocol):
        @property
        def value(self, fallback: T | None = None) -> int: ...
        @value.setter
        def value(self, value: int, fallback: T | None = None) -> None: ...

    class Holder:
        value: ClassVar[Property]  # no diagnostic
```

## Captured type variables in protocols

A local protocol can use a type variable from an enclosing function in an attribute or method.
`ClassVar` rejects these uses, even when the method also has type parameters of its own.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, Protocol

def outer[T]():
    class Captured(Protocol):
        value: T

    class Callback(Protocol):
        def method[U](self, value: U) -> tuple[T, U]: ...

    class Holder:
        # error: [invalid-type-form]
        attribute: ClassVar[Captured]
        # error: [invalid-type-form]
        method: ClassVar[Callback]
```

## Captured type variables in `TypedDict` fields

`ClassVar` also rejects a local `TypedDict` whose fields use an outer type variable.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, TypedDict

def outer[T]() -> None:
    class Payload(TypedDict):
        value: T

    class Holder:
        # error: [invalid-type-form]
        payload: ClassVar[Payload]
```

## Recursive protocols

Recursive protocols are valid when they contain no free type variables. A method's inferred `self`
type and its own type parameters do not make the protocol invalid.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

from typing import ClassVar, Protocol

class Recursive[T](Protocol):
    def method[U](self, value: U) -> tuple[Recursive[int], U]: ...

class Holder:
    protocol: ClassVar[Recursive[int]]  # no diagnostic
```

This remains true when each recursive reference adds a `list` around the type argument, starting
from `int`.

```py
def outer():
    class Recursive[T](Protocol):
        next: Recursive[list[T]]

    class Holder:
        protocol: ClassVar[Recursive[int]]  # no diagnostic
```

## Recursive protocols with different specializations

The `value` field of `Value[object]` simplifies to `object`. The second tuple element reaches
`Value[int]` through recursive references; its `value` field still contains the outer type variable
`T`. The `nested` member keeps adding lists to the type argument, so the first traversal is
incomplete. That must not prevent us from checking the second tuple element.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

from typing import ClassVar, Protocol

def outer[T]():
    class Value[U](Protocol):
        cycle: Link
        value: U | T
        nested: Value[list[U]]

    class Link(Protocol):
        cycle: BackLink
        value: Value[int]

    class BackLink(Protocol):
        cycle: Link

    class Holder:
        # error: [invalid-type-form]
        value: ClassVar[tuple[Value[object], BackLink]]
```

The same applies to a protocol declared with legacy type variables.

```py
from typing import TypeVar

T = TypeVar("T")
U = TypeVar("U")

def legacy_outer(value: T):
    class Value(Protocol[U]):
        cycle: Link
        value: U | T
        nested: Value[list[U]]

    class Link(Protocol):
        cycle: BackLink
        value: Value[int]

    class BackLink(Protocol):
        cycle: Link

    class Holder:
        # error: [invalid-type-form]
        value: ClassVar[tuple[Value[object], BackLink]]
```

## Recursive `TypedDict`s

A recursive `TypedDict` is valid when its fields contain no free type variables.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

from typing import ClassVar, TypedDict

class RecursivePayload(TypedDict):
    next: RecursivePayload

class PayloadHolder:
    payload: ClassVar[RecursivePayload]  # no diagnostic
```

This remains true when each recursive reference wraps the type argument in another `list`.

```py
def outer():
    class Payload[T](TypedDict):
        next: Payload[list[T]]

    class Holder:
        payload: ClassVar[Payload[int]]  # no diagnostic
```

## Assignments through generic aliases

Assignments through generic aliases still resolve class variables.

```py
from typing import ClassVar, Generic, TypeVar
from typing_extensions import reveal_type

T = TypeVar("T")

class Box(Generic[T]):
    count: ClassVar[int]

Box[int].count = 1
reveal_type(Box[int].count)  # revealed: int
```

## Combining `ClassVar` and `Final` in normal classes

An attribute on a class body that is annotated as `Final` is implicitly treated as a class variable.
The error message is different, but these attributes cannot be written to from instances of the
class:

```py
from typing import Final

class C:
    a: Final[int] = 1

reveal_type(C.a)  # revealed: int

c = C()
c.a = 2  # error: [invalid-assignment] "Cannot assign to final attribute `a` on type `C`"
```

In this sense, it is redundant to combine `ClassVar` and `Final`. We issue a warning in these cases:

```py
from typing import Annotated, ClassVar

class D:
    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    a: ClassVar[Final[int]] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    b: Final[ClassVar[int]] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    c: Final[ClassVar] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    d: Annotated[Final[ClassVar[int]], "metadata"] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    e: ClassVar[Final] = 1

    # error: [redundant-final-classvar] "Combining `ClassVar` and `Final` is redundant"
    f: Annotated[Final[Annotated[Annotated[ClassVar[int], "a"], "b"]], "c"] = 1

reveal_type(D.a)  # revealed: int
reveal_type(D.b)  # revealed: int
reveal_type(D.c)  # revealed: Literal[1]
reveal_type(D.d)  # revealed: int
reveal_type(D.e)  # revealed: Literal[1]
reveal_type(D.f)  # revealed: int

d = D()
d.a = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `a` from an instance of type `D`"
d.b = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `b` from an instance of type `D`"
d.c = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `c` from an instance of type `D`"
d.d = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `d` from an instance of type `D`"
d.e = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `e` from an instance of type `D`"
d.f = 2  # error: [invalid-attribute-access] "Cannot assign to ClassVar `f` from an instance of type `D`"
```

## Combining `ClassVar` and `Final` in dataclasses

In dataclasses, `ClassVar[Final[int]]` has a distinct meaning from `Final[int]`. The former is a
final class variable, the latter is a final instance attribute. The warning is therefore not emitted
when combining `ClassVar[Final[...]]` in dataclasses:

```py
from dataclasses import dataclass
from typing import ClassVar, Final

@dataclass
class D:
    # No warning:
    class_attr: ClassVar[Final[int]] = 1

    instance_attr: Final[int] = 1
```

Note that `class_attr` does not appear in the signature of `__init__`:

```py
# revealed: (self: D, instance_attr: int = 1) -> None
reveal_type(D.__init__)
```

```py
def _(d: D):
    reveal_type(d.class_attr)  # revealed: int
    reveal_type(d.instance_attr)  # revealed: int

    d.class_attr = 2  # error: [invalid-attribute-access]
```

The reverse direction `Final[ClassVar[...]]` is not recognized by the runtime implementation of
dataclasses. We could consider emitting a warning in these cases, but for now, we treat is just like
`ClassVar[Final[...]]` and allow it in dataclasses:

```py
from dataclasses import dataclass

@dataclass
class E:
    class_attr: Final[ClassVar[int]] = 1

# revealed: (self: E) -> None
reveal_type(E.__init__)

def _(e: E):
    reveal_type(e.class_attr)  # revealed: int

    e.class_attr = 2  # error: [invalid-attribute-access]
```

## Illegal `ClassVar` in type expression

```py
from typing import ClassVar

class C:
    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
    x: ClassVar | int

    # error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
    y: int | ClassVar[str]
```

## Illegal positions

```toml
[environment]
python-version = "3.12"
```

```py
from typing import ClassVar, TypedDict
from ty_extensions._internal import reveal_mro

# error: [invalid-type-form] "`ClassVar` is only allowed in class bodies"
x: ClassVar[int] = 1

class C:
    def __init__(self) -> None:
        # error: [invalid-type-form] "`ClassVar` annotations are not allowed for non-name targets"
        self.x: ClassVar[int] = 1

        # error: [invalid-type-form] "`ClassVar` is only allowed in class bodies"
        y: ClassVar[int] = 1

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in parameter annotations"
def f(x: ClassVar[int]) -> None:
    pass

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in parameter annotations"
def f[T](x: ClassVar[T]) -> T:
    return x

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in return type annotations"
def f() -> ClassVar[int]:
    return 1

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not allowed in return type annotations"
def f[T](x: T) -> ClassVar[T]:
    return x

# TODO: this should be an error
class Foo(ClassVar[tuple[int]]): ...

# TODO: Show `Unknown` instead of `@Todo` type in the MRO; or ignore `ClassVar` and show the MRO as if `ClassVar` was not there
# revealed: (<class 'Foo'>, @Todo(Inference of subscript on special form), <class 'object'>)
reveal_mro(Foo)

class Foo(TypedDict):
    # error: [invalid-type-form] "`ClassVar` is not allowed in TypedDict fields"
    x: ClassVar[int]
    # error: [invalid-type-form] "`ClassVar` is not allowed in TypedDict fields"
    y: ClassVar
```

[`typing.classvar`]: https://docs.python.org/3/library/typing.html#typing.ClassVar
