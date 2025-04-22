# Protocols

> [!NOTE]
>
> See also:
>
> - The [typing specification section on protocols][typing_spec_protocols]
> - The many [protocol conformance tests] provided by the Typing Council for type checkers
> - Mypy's [documentation][mypy_protocol_docs] and [tests][mypy_protocol_tests] for protocols

Most types in Python are *nominal* types: a fully static nominal type `X` is only a subtype of
another fully static nominal type `Y` if the class `X` is a subclass of the class `Y`.
`typing.Protocol` (or its backport, `typing_extensions.Protocol`) can be used to define *structural*
types, on the other hand: a type which is defined by its properties and behaviour.

## Defining a protocol

```toml
[environment]
python-version = "3.12"
```

A protocol is defined by inheriting from the `Protocol` class, which is annotated as an instance of
`_SpecialForm` in typeshed's stubs.

```py
from typing import Protocol

class MyProtocol(Protocol): ...

reveal_type(MyProtocol.__mro__)  # revealed: tuple[Literal[MyProtocol], typing.Protocol, typing.Generic, Literal[object]]
```

Just like for any other class base, it is an error for `Protocol` to appear multiple times in a
class's bases:

```py
class Foo(Protocol, Protocol): ...  # error: [inconsistent-mro]

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]
```

Protocols can also be generic, either by including `Generic[]` in the bases list, subscripting
`Protocol` directly in the bases list, using PEP-695 type parameters, or some combination of the
above:

```py
from typing import TypeVar, Generic

T = TypeVar("T")

class Bar0(Protocol[T]):
    x: T

class Bar1(Protocol[T], Generic[T]):
    x: T

class Bar2[T](Protocol):
    x: T

class Bar3[T](Protocol[T]):
    x: T
```

It's an error to include both bare `Protocol` and subscripted `Protocol[]` in the bases list
simultaneously:

```py
# TODO: should emit a `[duplicate-bases]` error here:
class DuplicateBases(Protocol, Protocol[T]):
    x: T

# TODO: should not have `Generic` multiple times and `Protocol` multiple times
# revealed: tuple[Literal[DuplicateBases], typing.Protocol, typing.Generic, @Todo(`Protocol[]` subscript), @Todo(`Generic[]` subscript), Literal[object]]
reveal_type(DuplicateBases.__mro__)
```

The introspection helper `typing(_extensions).is_protocol` can be used to verify whether a class is
a protocol class or not:

```py
from typing_extensions import is_protocol

reveal_type(is_protocol(MyProtocol))  # revealed: Literal[True]
reveal_type(is_protocol(Bar0))  # revealed: Literal[True]
reveal_type(is_protocol(Bar1))  # revealed: Literal[True]
reveal_type(is_protocol(Bar2))  # revealed: Literal[True]
reveal_type(is_protocol(Bar3))  # revealed: Literal[True]

class NotAProtocol: ...

reveal_type(is_protocol(NotAProtocol))  # revealed: Literal[False]
```

A type checker should follow the typeshed stubs if a non-class is passed in, and typeshed's stubs
indicate that the argument passed in must be an instance of `type`.

```py
# We could also reasonably infer `Literal[False]` here, but it probably doesn't matter that much:
# error: [invalid-argument-type]
reveal_type(is_protocol("not a class"))  # revealed: bool
```

For a class to be considered a protocol class, it must have `Protocol` directly in its bases tuple:
it is not sufficient for it to have `Protocol` in its MRO.

```py
class SubclassOfMyProtocol(MyProtocol): ...

# revealed: tuple[Literal[SubclassOfMyProtocol], Literal[MyProtocol], typing.Protocol, typing.Generic, Literal[object]]
reveal_type(SubclassOfMyProtocol.__mro__)

reveal_type(is_protocol(SubclassOfMyProtocol))  # revealed: Literal[False]
```

A protocol class may inherit from other protocols, however, as long as it re-inherits from
`Protocol`:

```py
class SubProtocol(MyProtocol, Protocol): ...

reveal_type(is_protocol(SubProtocol))  # revealed: Literal[True]

class OtherProtocol(Protocol):
    some_attribute: str

class ComplexInheritance(SubProtocol, OtherProtocol, Protocol): ...

# revealed: tuple[Literal[ComplexInheritance], Literal[SubProtocol], Literal[MyProtocol], Literal[OtherProtocol], typing.Protocol, typing.Generic, Literal[object]]
reveal_type(ComplexInheritance.__mro__)

reveal_type(is_protocol(ComplexInheritance))  # revealed: Literal[True]
```

If `Protocol` is present in the bases tuple, all other bases in the tuple must be protocol classes,
or `TypeError` is raised at runtime when the class is created.

```py
# error: [invalid-protocol] "Protocol class `Invalid` cannot inherit from non-protocol class `NotAProtocol`"
class Invalid(NotAProtocol, Protocol): ...

# revealed: tuple[Literal[Invalid], Literal[NotAProtocol], typing.Protocol, typing.Generic, Literal[object]]
reveal_type(Invalid.__mro__)

# error: [invalid-protocol] "Protocol class `AlsoInvalid` cannot inherit from non-protocol class `NotAProtocol`"
class AlsoInvalid(MyProtocol, OtherProtocol, NotAProtocol, Protocol): ...

# revealed: tuple[Literal[AlsoInvalid], Literal[MyProtocol], Literal[OtherProtocol], Literal[NotAProtocol], typing.Protocol, typing.Generic, Literal[object]]
reveal_type(AlsoInvalid.__mro__)
```

But two exceptions to this rule are `object` and `Generic`:

```py
from typing import TypeVar, Generic

T = TypeVar("T")

# Note: pyright and pyrefly do not consider this to be a valid `Protocol` class,
# but mypy does (and has an explicit test for this behaviour). Mypy was the
# reference implementation for PEP-544, and its behaviour also matches the CPython
# runtime, so we choose to follow its behaviour here rather than that of the other
# type checkers.
class Fine(Protocol, object): ...

reveal_type(Fine.__mro__)  # revealed: tuple[Literal[Fine], typing.Protocol, typing.Generic, Literal[object]]

class StillFine(Protocol, Generic[T], object): ...
class EvenThis[T](Protocol, object): ...
class OrThis(Protocol[T], Generic[T]): ...
class AndThis(Protocol[T], Generic[T], object): ...
```

And multiple inheritance from a mix of protocol and non-protocol classes is fine as long as
`Protocol` itself is not in the bases list:

```py
class FineAndDandy(MyProtocol, OtherProtocol, NotAProtocol): ...

# revealed: tuple[Literal[FineAndDandy], Literal[MyProtocol], Literal[OtherProtocol], typing.Protocol, typing.Generic, Literal[NotAProtocol], Literal[object]]
reveal_type(FineAndDandy.__mro__)
```

But if `Protocol` is not present in the bases list, the resulting class doesn't count as a protocol
class anymore:

```py
reveal_type(is_protocol(FineAndDandy))  # revealed: Literal[False]
```

A class does not *have* to inherit from a protocol class in order for it to be considered a subtype
of that protocol (more on that below). However, classes that explicitly inherit from a protocol
class are understood as subtypes of that protocol, the same as with nominal types:

```py
from knot_extensions import static_assert, is_subtype_of, is_assignable_to

static_assert(is_subtype_of(SubclassOfMyProtocol, MyProtocol))
static_assert(is_assignable_to(SubclassOfMyProtocol, MyProtocol))

static_assert(is_subtype_of(SubProtocol, MyProtocol))
static_assert(is_assignable_to(SubProtocol, MyProtocol))

static_assert(is_subtype_of(ComplexInheritance, SubProtocol))
static_assert(is_assignable_to(ComplexInheritance, SubProtocol))

static_assert(is_subtype_of(ComplexInheritance, OtherProtocol))
static_assert(is_assignable_to(ComplexInheritance, SubProtocol))

static_assert(is_subtype_of(FineAndDandy, MyProtocol))
static_assert(is_assignable_to(FineAndDandy, MyProtocol))

static_assert(is_subtype_of(FineAndDandy, OtherProtocol))
static_assert(is_assignable_to(FineAndDandy, OtherProtocol))
```

Note, however, that `Protocol` itself is not a type, so it is an error to pass it to `is_subtype_of`
or `is_assignable_to`:

```py
is_subtype_of(MyProtocol, Protocol)  # error: [invalid-type-form]
is_assignable_to(MyProtocol, Protocol)  # error: [invalid-type-form]
```

And it is also an error to use `Protocol` in type expressions:

```py
# fmt: off

def f(
    x: Protocol,  # error: [invalid-type-form] "`typing.Protocol` is not allowed in type expressions"
    y: type[Protocol],  # TODO: should emit `[invalid-type-form]` here too
) -> None:
    reveal_type(x)  # revealed: Unknown

    # TODO: should be `type[Unknown]`
    reveal_type(y)  # revealed: @Todo(unsupported type[X] special form)

# fmt: on
```

Nonetheless, `Protocol` can still be used as the second argument to `issubclass()` at runtime:

```py
# TODO: should be `Literal[True]`
reveal_type(issubclass(MyProtocol, Protocol))  # revealed: bool
```

## `typing.Protocol` versus `typing_extensions.Protocol`

`typing.Protocol` and its backport in `typing_extensions` should be treated as exactly equivalent.

```py
import typing
import typing_extensions
from knot_extensions import static_assert, is_equivalent_to, TypeOf

static_assert(is_equivalent_to(TypeOf[typing.Protocol], TypeOf[typing_extensions.Protocol]))
static_assert(is_equivalent_to(int | str | TypeOf[typing.Protocol], TypeOf[typing_extensions.Protocol] | str | int))

class Foo(typing.Protocol):
    x: int

class Bar(typing_extensions.Protocol):
    x: int

static_assert(typing_extensions.is_protocol(Foo))
static_assert(typing_extensions.is_protocol(Bar))

# TODO: should pass
static_assert(is_equivalent_to(Foo, Bar))  # error: [static-assert-error]
```

The same goes for `typing.runtime_checkable` and `typing_extensions.runtime_checkable`:

```py
@typing_extensions.runtime_checkable
class RuntimeCheckableFoo(typing.Protocol):
    x: int

@typing.runtime_checkable
class RuntimeCheckableBar(typing_extensions.Protocol):
    x: int

static_assert(typing_extensions.is_protocol(RuntimeCheckableFoo))
static_assert(typing_extensions.is_protocol(RuntimeCheckableBar))

# TODO: should pass
static_assert(is_equivalent_to(RuntimeCheckableFoo, RuntimeCheckableBar))  # error: [static-assert-error]

# These should not error because the protocols are decorated with `@runtime_checkable`
isinstance(object(), RuntimeCheckableFoo)
isinstance(object(), RuntimeCheckableBar)
```

However, we understand that they are not necessarily the same symbol at the same memory address at
runtime -- these reveal `bool` rather than `Literal[True]` or `Literal[False]`, which would be
incorrect:

```py
reveal_type(typing.Protocol is typing_extensions.Protocol)  # revealed: bool
reveal_type(typing.Protocol is not typing_extensions.Protocol)  # revealed: bool
```

## Calls to protocol classes

Neither `Protocol`, nor any protocol class, can be directly instantiated:

```py
from typing import Protocol

# error: [call-non-callable]
reveal_type(Protocol())  # revealed: Unknown

class MyProtocol(Protocol):
    x: int

# error
reveal_type(MyProtocol())  # revealed: MyProtocol
```

But a non-protocol class can be instantiated, even if it has `Protocol` in its MRO:

```py
class SubclassOfMyProtocol(MyProtocol): ...

reveal_type(SubclassOfMyProtocol())  # revealed: SubclassOfMyProtocol
```

And as a corollary, `type[MyProtocol]` can also be called:

```py
def f(x: type[MyProtocol]):
    reveal_type(x())  # revealed: MyProtocol
```

## Members of a protocol

A protocol defines an interface through its *members*: if a protocol `Foo` has members `X` and `Y`,
a type `Bar` can only be a subtype of `Foo` if inhabitants of `Bar` also have attributes `X` and
`Y`.

A protocol class defines its members through declarations in the class body. The members of a
protocol can be introspected using the function `typing.get_protocol_members`, which is backported
via `typing_extensions`.

```py
from typing_extensions import Protocol, get_protocol_members

class Foo(Protocol):
    x: int

    @property
    def y(self) -> str:
        return "y"

    @property
    def z(self) -> int:
        return 42

    @z.setter
    def z(self, z: int) -> None: ...
    def method_member(self) -> bytes:
        return b"foo"

# TODO: at runtime, `get_protocol_members` returns a `frozenset`,
# but for now we might pretend it returns a `tuple`, as we support heterogeneous `tuple` types
# but not yet generic `frozenset`s
#
# So this should either be
#
# `tuple[Literal["x"], Literal["y"], Literal["z"], Literal["method_member"]]`
#
# `frozenset[Literal["x", "y", "z", "method_member"]]`
reveal_type(get_protocol_members(Foo))  # revealed: @Todo(specialized non-generic class)
```

Certain special attributes and methods are not considered protocol members at runtime, and should
not be considered protocol members by type checkers either:

```py
class Lumberjack(Protocol):
    __slots__ = ()
    __match_args__ = ()
    x: int

    def __new__(cls, x: int) -> "Lumberjack":
        return object.__new__(cls)

    def __init__(self, x: int) -> None:
        self.x = x

# TODO: `tuple[Literal["x"]]` or `frozenset[Literal["x"]]`
reveal_type(get_protocol_members(Lumberjack))  # revealed: @Todo(specialized non-generic class)
```

A sub-protocol inherits and extends the members of its superclass protocol(s):

```py
class Bar(Protocol):
    spam: str

class Baz(Bar, Protocol):
    ham: memoryview

# TODO: `tuple[Literal["spam", "ham"]]` or `frozenset[Literal["spam", "ham"]]`
reveal_type(get_protocol_members(Baz))  # revealed: @Todo(specialized non-generic class)

class Baz2(Bar, Foo, Protocol): ...

# TODO: either
# `tuple[Literal["spam"], Literal["x"], Literal["y"], Literal["z"], Literal["method_member"]]`
# or `frozenset[Literal["spam", "x", "y", "z", "method_member"]]`
reveal_type(get_protocol_members(Baz2))  # revealed: @Todo(specialized non-generic class)
```

## Invalid calls to `get_protocol_members()`

<!-- snapshot-diagnostics -->

Calling `get_protocol_members` on a non-protocol class raises an error at runtime:

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import Protocol, get_protocol_members

class NotAProtocol: ...

get_protocol_members(NotAProtocol)  # error: [invalid-argument-type]

class AlsoNotAProtocol(NotAProtocol, object): ...

get_protocol_members(AlsoNotAProtocol)  # error: [invalid-argument-type]
```

The original class object must be passed to the function; a specialised version of a generic version
does not suffice:

```py
class GenericProtocol[T](Protocol): ...

get_protocol_members(GenericProtocol[int])  # TODO: should emit a diagnostic here (https://github.com/astral-sh/ruff/issues/17549)
```

## Subtyping of protocols with attribute members

In the following example, the protocol class `HasX` defines an interface such that any other fully
static type can be said to be a subtype of `HasX` if all inhabitants of that other type have a
mutable `x` attribute of type `int`:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol
from knot_extensions import static_assert, is_assignable_to, is_subtype_of

class HasX(Protocol):
    x: int

class Foo:
    x: int

# TODO: these should pass
static_assert(is_subtype_of(Foo, HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(Foo, HasX))  # error: [static-assert-error]

class FooSub(Foo): ...

# TODO: these should pass
static_assert(is_subtype_of(FooSub, HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(FooSub, HasX))  # error: [static-assert-error]

class Bar:
    x: str

static_assert(not is_subtype_of(Bar, HasX))
static_assert(not is_assignable_to(Bar, HasX))

class Baz:
    y: int

static_assert(not is_subtype_of(Baz, HasX))
static_assert(not is_assignable_to(Baz, HasX))
```

Note that declaring an attribute member on a protocol mandates that the attribute must be mutable. A
type with a read-only `x` property does not satisfy the `HasX` interface; nor does a type with a
`Final` `x` attribute. The type of the attribute must also be treated as invariant due to the
attribute's mutability:

```py
from typing import Final

class A:
    @property
    def x(self) -> int:
        return 42

static_assert(not is_subtype_of(A, HasX))
static_assert(not is_assignable_to(A, HasX))

class B:
    x: Final = 42

static_assert(not is_subtype_of(A, HasX))
static_assert(not is_assignable_to(A, HasX))

class IntSub(int): ...

class C:
    x: IntSub

# due to invariance, a type is only a subtype of `HasX`
# if its `x` attribute is of type *exactly* `int`:
# a subclass of `int` does not satisfy the interface
static_assert(not is_subtype_of(C, HasX))
static_assert(not is_assignable_to(C, HasX))
```

All attributes on frozen dataclasses and namedtuples are immutable, so instances of these classes
can never be considered to inhabit a protocol that declares a mutable-attribute member:

```py
from dataclasses import dataclass
from typing import NamedTuple

@dataclass
class MutableDataclass:
    x: int

# TODO: these should pass
static_assert(is_subtype_of(MutableDataclass, HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(MutableDataclass, HasX))  # error: [static-assert-error]

@dataclass(frozen=True)
class ImmutableDataclass:
    x: int

static_assert(not is_subtype_of(ImmutableDataclass, HasX))
static_assert(not is_assignable_to(ImmutableDataclass, HasX))

class NamedTupleWithX(NamedTuple):
    x: int

static_assert(not is_subtype_of(NamedTupleWithX, HasX))
static_assert(not is_assignable_to(NamedTupleWithX, HasX))
```

However, a type with a read-write property `x` *does* satisfy the `HasX` protocol. The `HasX`
protocol only specifies what the type of `x` should be when accessed from instances; instances of
`XProperty` in the below example have a mutable attribute `x` of type `int`:

```py
class XProperty:
    _x: int

    @property
    def x(self) -> int:
        return self._x

    @x.setter
    def x(self, x: int) -> None:
        self._x = x**2

# TODO: these should pass
static_assert(is_subtype_of(XProperty, HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(XProperty, HasX))  # error: [static-assert-error]
```

Attribute members on protocol classes are allowed to have default values, just like instance
attributes on other classes. Similar to nominal classes, attributes with defaults can be accessed on
the class object itself and any explicit subclasses of the protocol class. It cannot be assumed to
exist on the meta-type of any arbitrary inhabitant of the protocol type, however; an implicit
subtype of the protocol will not necessarily have a default value for the instance attribute
provided in its class body:

```py
class HasXWithDefault(Protocol):
    x: int = 42

reveal_type(HasXWithDefault.x)  # revealed: int

class ExplicitSubclass(HasXWithDefault): ...

reveal_type(ExplicitSubclass.x)  # revealed: int

def f(arg: HasXWithDefault):
    # TODO: should emit `[unresolved-reference]` and reveal `Unknown`
    reveal_type(type(arg).x)  # revealed: int
```

Assignments in a class body of a protocol -- of any kind -- are not permitted by red-knot unless the
symbol being assigned to is also explicitly declared in the protocol's class body. Note that this is
stricter validation of protocol members than many other type checkers currently apply (as of
2025/04/21).

The reason for this strict validation is that undeclared variables in the class body would lead to
an ambiguous interface being declared by the protocol.

```py
from typing_extensions import TypeAlias, get_protocol_members

class MyContext:
    def __enter__(self) -> int:
        return 42

    def __exit__(self, *args) -> None: ...

class LotsOfBindings(Protocol):
    a: int
    a = 42  # this is fine, since `a` is declared in the class body
    b: int = 56  # this is also fine, by the same principle

    type c = str  # this is very strange but I can't see a good reason to disallow it
    d: TypeAlias = bytes  # same here

    class Nested: ...  # also weird, but we should also probably allow it
    class NestedProtocol(Protocol): ...  # same here...
    e = 72  # TODO: this should error with `[invalid-protocol]` (`e` is not declared)

    f, g = (1, 2)  # TODO: this should error with `[invalid-protocol]` (`f` and `g` are not declared)

    h: int = (i := 3)  # TODO: this should error with `[invalid-protocol]` (`i` is not declared)

    for j in range(42):  # TODO: this should error with `[invalid-protocol]` (`j` is not declared)
        pass

    with MyContext() as k:  # TODO: this should error with `[invalid-protocol]` (`k` is not declared)
        pass

    match object():
        case l:  # TODO: this should error with `[invalid-protocol]` (`l` is not declared)
            ...

# TODO: all bindings in the above class should be understood as protocol members,
# even those that we complained about with a diagnostic
reveal_type(get_protocol_members(LotsOfBindings))  # revealed: @Todo(specialized non-generic class)
```

Attribute members are allowed to have assignments in methods on the protocol class, just like
non-protocol classes. Unlike other classes, however, *implicit* instance attributes -- those that
are not declared in the class body -- are not allowed:

```py
class Foo(Protocol):
    x: int
    y: str

    def __init__(self) -> None:
        self.x = 42  # fine
        self.a = 56  # error

    def non_init_method(self) -> None:
        self.y = 64  # fine
        self.b = 72  # error
```

If a protocol has 0 members, then all other types are assignable to it, and all fully static types
are subtypes of it:

```py
from typing import Protocol

class UniversalSet(Protocol): ...

# TODO: these should pass
static_assert(is_assignable_to(object, UniversalSet))  # error: [static-assert-error]
static_assert(is_subtype_of(object, UniversalSet))  # error: [static-assert-error]
```

Which means that `UniversalSet` here is in fact an equivalent type to `object`:

```py
from knot_extensions import is_equivalent_to

# TODO: this should pass
static_assert(is_equivalent_to(UniversalSet, object))  # error: [static-assert-error]
```

`object` is a subtype of certain other protocols too. Since all fully static types (whether nominal
or structural) are subtypes of `object`, these protocols are also subtypes of `object`; and this
means that these protocols are also equivalent to `UniversalSet` and `object`:

```py
class SupportsStr(Protocol):
    def __str__(self) -> str: ...

# TODO: these should pass
static_assert(is_equivalent_to(SupportsStr, UniversalSet))  # error: [static-assert-error]
static_assert(is_equivalent_to(SupportsStr, object))  # error: [static-assert-error]

class SupportsClass(Protocol):
    __class__: type

# TODO: these should pass
static_assert(is_equivalent_to(SupportsClass, UniversalSet))  # error: [static-assert-error]
static_assert(is_equivalent_to(SupportsClass, SupportsStr))  # error: [static-assert-error]
static_assert(is_equivalent_to(SupportsClass, object))  # error: [static-assert-error]
```

If a protocol contains members that are not defined on `object`, then that protocol will (like all
types in Python) still be assignable to `object`, but `object` will not be assignable to that
protocol:

```py
static_assert(is_assignable_to(HasX, object))
static_assert(is_subtype_of(HasX, object))
static_assert(not is_assignable_to(object, HasX))
static_assert(not is_subtype_of(object, HasX))
```

But `object` is the *only* fully static nominal type that a protocol type can ever be assignable to
or a subtype of:

```py
static_assert(not is_assignable_to(HasX, Foo))
static_assert(not is_subtype_of(HasX, Foo))
```

## Equivalence of protocols

Two protocols are considered equivalent types if they specify the same interface, even if they have
different names:

```py
from typing import Protocol
from knot_extensions import is_equivalent_to, static_assert

class HasX(Protocol):
    x: int

class AlsoHasX(Protocol):
    x: int

# TODO: this should pass
static_assert(is_equivalent_to(HasX, AlsoHasX))  # error: [static-assert-error]
```

And unions containing equivalent protocols are recognised as equivalent, even when the order is not
identical:

```py
class HasY(Protocol):
    y: str

class AlsoHasY(Protocol):
    y: str

class A: ...
class B: ...

# TODO: this should pass
static_assert(is_equivalent_to(A | HasX | B | HasY, B | AlsoHasY | AlsoHasX | A))  # error: [static-assert-error]
```

## Intersections of protocols

An intersection of two protocol types `X` and `Y` is equivalent to a protocol type `Z` that inherits
from both `X` and `Y`:

```py
from typing import Protocol
from knot_extensions import Intersection, static_assert, is_equivalent_to

class HasX(Protocol):
    x: int

class HasY(Protocol):
    y: str

class HasXAndYProto(HasX, HasY, Protocol): ...

# TODO: this should pass
static_assert(is_equivalent_to(HasXAndYProto, Intersection[HasX, HasY]))  # error: [static-assert-error]
```

But this is only true if the subclass has `Protocol` in its explicit bases (otherwise, it is a
nominal type rather than a structural type):

```py
class HasXAndYNominal(HasX, HasY): ...

static_assert(not is_equivalent_to(HasXAndYNominal, Intersection[HasX, HasY]))
```

A protocol type `X` and a nominal type `Y` can be inferred as disjoint types if `Y` is a `@final`
type and `Y` does not satisfy the interface declared by `X`. But if `Y` is not `@final`, then this
does not hold true, since a subclass of `Y` could always provide additional methods or attributes
that would lead to it satisfying `X`'s interface:

```py
from typing import final
from knot_extensions import is_disjoint_from

class NotFinalNominal: ...

@final
class FinalNominal: ...

static_assert(not is_disjoint_from(NotFinalNominal, HasX))
static_assert(is_disjoint_from(FinalNominal, HasX))

def _(arg1: Intersection[HasX, NotFinalNominal], arg2: Intersection[HasX, FinalNominal]):
    reveal_type(arg1)  # revealed: HasX & NotFinalNominal
    reveal_type(arg2)  # revealed: Never
```

## Satisfying a protocol's interface

A type does not have to be an `Instance` type in order to be a subtype of a protocol. Other
protocols can be a subtype of a protocol, as can `ModuleLiteral` types, `ClassLiteral` types, and
others. Another protocol can be a subtype of `HasX` either through "explicit" (nominal) inheritance
from `HasX`, or by specifying a superset of `HasX`'s interface:

`module.py`:

```py
x: int = 42
```

`main.py`:

```py
import module
from typing import Protocol
from knot_extensions import is_subtype_of, is_assignable_to, static_assert, TypeOf

class HasX(Protocol):
    x: int

# TODO: these should pass
static_assert(is_subtype_of(TypeOf[module], HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(TypeOf[module], HasX))  # error: [static-assert-error]

class ExplicitProtocolSubtype(HasX, Protocol):
    y: int

static_assert(is_subtype_of(ExplicitProtocolSubtype, HasX))
static_assert(is_assignable_to(ExplicitProtocolSubtype, HasX))

class ImplicitProtocolSubtype(Protocol):
    x: int
    y: str

# TODO: these should pass
static_assert(is_subtype_of(ImplicitProtocolSubtype, HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(ImplicitProtocolSubtype, HasX))  # error: [static-assert-error]

class Meta(type):
    x: int

class UsesMeta(metaclass=Meta): ...

# TODO: these should pass
static_assert(is_subtype_of(UsesMeta, HasX))  # error: [static-assert-error]
static_assert(is_assignable_to(UsesMeta, HasX))  # error: [static-assert-error]
```

## `ClassVar` attribute members

If a protocol `ClassVarX` has a `ClassVar` attribute member `x` with type `int`, this indicates that
a readable `x` attribute must be accessible on any inhabitant of `ClassVarX`, and that a readable
`x` attribute must *also* be accessible on the *type* of that inhabitant:

`classvars.py`:

```py
from typing import ClassVar, Protocol
from knot_extensions import is_subtype_of, is_assignable_to, static_assert

class ClassVarXProto(Protocol):
    x: ClassVar[int]

def f(obj: ClassVarXProto):
    reveal_type(obj.x)  # revealed: int
    reveal_type(type(obj).x)  # revealed: int
    obj.x = 42  # error: [invalid-attribute-access] "Cannot assign to ClassVar `x` from an instance of type `ClassVarXProto`"

class InstanceAttrX:
    x: int

static_assert(not is_assignable_to(InstanceAttrX, ClassVarXProto))
static_assert(not is_subtype_of(InstanceAttrX, ClassVarXProto))

class PropertyX:
    @property
    def x(self) -> int:
        return 42

static_assert(not is_assignable_to(PropertyX, ClassVarXProto))
static_assert(not is_subtype_of(PropertyX, ClassVarXProto))

class ClassVarX:
    x: ClassVar[int] = 42

# TODO: these should pass
static_assert(is_assignable_to(ClassVarX, ClassVarXProto))  # error: [static-assert-error]
static_assert(is_subtype_of(ClassVarX, ClassVarXProto))  # error: [static-assert-error]
```

This is mentioned by the
[spec](https://typing.python.org/en/latest/spec/protocol.html#protocol-members) and tested in the
[conformance suite](https://github.com/python/typing/blob/main/conformance/tests/protocols_definition.py)
as something that must be supported by type checkers:

> To distinguish between protocol class variables and protocol instance variables, the special
> `ClassVar` annotation should be used.

## Subtyping of protocols with property members

A read-only property on a protocol can be satisfied by a mutable attribute, a read-only property, a
read/write property, a `Final` attribute, or a `ClassVar` attribute:

```py
from typing import ClassVar, Final, Protocol
from knot_extensions import is_subtype_of, is_assignable_to, static_assert

class HasXProperty(Protocol):
    @property
    def x(self) -> int: ...

class XAttr:
    x: int

# TODO: these should pass
static_assert(is_subtype_of(XAttr, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XAttr, HasXProperty))  # error: [static-assert-error]

class XReadProperty:
    @property
    def x(self) -> int:
        return 42

# TODO: these should pass
static_assert(is_subtype_of(XReadProperty, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XReadProperty, HasXProperty))  # error: [static-assert-error]

class XReadWriteProperty:
    @property
    def x(self) -> int:
        return 42

    @x.setter
    def x(self, val: int) -> None: ...

# TODO: these should pass
static_assert(is_subtype_of(XReadWriteProperty, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XReadWriteProperty, HasXProperty))  # error: [static-assert-error]

class XClassVar:
    x: ClassVar[int] = 42

static_assert(is_subtype_of(XClassVar, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XClassVar, HasXProperty))  # error: [static-assert-error]

class XFinal:
    x: Final = 42

# TODO: these should pass
static_assert(is_subtype_of(XFinal, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XFinal, HasXProperty))  # error: [static-assert-error]
```

A read-only property on a protocol, unlike a mutable attribute, is covariant: `XSub` in the below
example satisfies the `HasXProperty` interface even though the type of the `x` attribute on `XSub`
is a subtype of `int` rather than being exactly `int`.

```py
class MyInt(int): ...

class XSub:
    x: MyInt

# TODO: these should pass
static_assert(is_subtype_of(XSub, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XSub, HasXProperty))  # error: [static-assert-error]
```

A read/write property on a protocol, where the getter returns the same type that the setter takes,
is equivalent to a normal mutable attribute on a protocol.

```py
class HasMutableXProperty(Protocol):
    @property
    def x(self) -> int: ...
    @x.setter
    def x(self, val: int) -> None: ...

class XAttr:
    x: int

# TODO: these should pass
static_assert(is_subtype_of(XAttr, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XAttr, HasXProperty))  # error: [static-assert-error]

class XReadProperty:
    @property
    def x(self) -> int:
        return 42

static_assert(not is_subtype_of(XReadProperty, HasXProperty))
static_assert(not is_assignable_to(XReadProperty, HasXProperty))

class XReadWriteProperty:
    @property
    def x(self) -> int:
        return 42

    @x.setter
    def x(self, val: int) -> None: ...

# TODO: these should pass
static_assert(is_subtype_of(XReadWriteProperty, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XReadWriteProperty, HasXProperty))  # error: [static-assert-error]

class XSub:
    x: MyInt

static_assert(not is_subtype_of(XSub, HasXProperty))
static_assert(not is_assignable_to(XSub, HasXProperty))
```

A protocol with a read/write property `x` is exactly equivalent to a protocol with a mutable
attribute `x`. Both are subtypes of a protocol with a read-only prooperty `x`:

```py
from knot_extensions import is_equivalent_to

class HasMutableXAttr(Protocol):
    x: int

# TODO: this should pass
static_assert(is_equivalent_to(HasMutableXAttr, HasMutableXProperty))  # error: [static-assert-error]

# TODO: these should pass
static_assert(is_subtype_of(HasMutableXAttr, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasMutableXAttr, HasXProperty))  # error: [static-assert-error]

# TODO: these should pass
static_assert(is_subtype_of(HasMutableXProperty, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasMutableXProperty, HasXProperty))  # error: [static-assert-error]
```

A read/write property on a protocol, where the setter accepts a subtype of the type returned by the
getter, can be satisfied by a mutable attribute of any type bounded by the upper bound of the
getter-returned type and the lower bound of the setter-accepted type.

This follows from the principle that a type `X` can only be a subtype of a given protocol if the
`X`'s behaviour is a superset of the behaviour specified by the interface declared by the protocol.
In the below example, the behaviour of an instance of `XAttr` is a superset of the behaviour
specified by the protocol `HasAsymmetricXProperty`. The protocol specifies that reading an `x`
attribute on the instance must resolve to an instance of `int` or a subclass thereof, and `XAttr`
satisfies this requirement. The protocol also specifies that you must be able to assign instances of
`MyInt` to the `x` attribute, and again this is satisfied by `XAttr`: on instances of `XAttr`, you
can assign *any* instance of `int` to the `x` attribute, and thus by extension you can assign any
instance of `IntSub` to the `x` attribute, since any instance of `IntSub` is an instance of `int`:

```py
class HasAsymmetricXProperty(Protocol):
    @property
    def x(self) -> int: ...
    @x.setter
    def x(self, val: MyInt) -> None: ...

class XAttr:
    x: int

# TODO: these should pass
static_assert(is_subtype_of(XAttr, HasAsymmetricXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XAttr, HasAsymmetricXProperty))  # error: [static-assert-error]
```

The end conclusion of this is that the getter-returned type of a property is always covariant and
the setter-accepted type is always contravariant. The combination of these leads to invariance for a
regular mutable attribute, where the implied getter-returned and setter-accepted types are the same.

```py
class XAttrSub:
    x: MyInt

# TODO: these should pass
static_assert(is_subtype_of(XAttrSub, HasAsymmetricXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XAttrSub, HasAsymmetricXProperty))  # error: [static-assert-error]

class MyIntSub(MyInt):
    pass

class XAttrSubSub:
    x: MyIntSub

static_assert(not is_subtype_of(XAttrSubSub, HasAsymmetricXProperty))
static_assert(not is_assignable_to(XAttrSubSub, HasAsymmetricXProperty))
```

An asymmetric property on a protocol can also be satisfied by an asymmetric property on a nominal
class whose getter and setter types satisfy the covariant and contravariant requirements,
respectively.

```py
class XAsymmetricProperty:
    @property
    def x(self) -> MyInt:
        return MyInt(0)

    @x.setter
    def x(self, x: int) -> None: ...

# TODO: these should pass
static_assert(is_subtype_of(XAsymmetricProperty, HasAsymmetricXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XAsymmetricProperty, HasAsymmetricXProperty))  # error: [static-assert-error]
```

A custom descriptor attribute on the nominal class will also suffice:

```py
class Descriptor:
    def __get__(self, instance, owner) -> MyInt:
        return MyInt(0)

    def __set__(self, value: int) -> None: ...

class XCustomDescriptor:
    x: Descriptor = Descriptor()

# TODO: these should pass
static_assert(is_subtype_of(XCustomDescriptor, HasAsymmetricXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(XCustomDescriptor, HasAsymmetricXProperty))  # error: [static-assert-error]
```

Moreover, a read-only property on a protocol can be satisfied by a nominal class that defines a
`__getattr__` method returning a suitable type. A read/write property can be satisfied by a nominal
class that defines a `__getattr__` method returning a suitable type *and* a `__setattr__` method
accepting a suitable type:

```py
class HasGetAttr:
    def __getattr__(self, attr: str) -> int:
        return 42

# TODO: these should pass
static_assert(is_subtype_of(HasGetAttr, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasGetAttr, HasXProperty))  # error: [static-assert-error]

static_assert(not is_subtype_of(HasGetAttr, HasMutableXAttr))
static_assert(not is_subtype_of(HasGetAttr, HasMutableXAttr))

class HasGetAttrWithUnsuitableReturn:
    def __getattr__(self, attr: str) -> tuple[int, int]:
        return (1, 2)

static_assert(not is_subtype_of(HasGetAttrWithUnsuitableReturn, HasXProperty))
static_assert(not is_assignable_to(HasGetAttrWithUnsuitableReturn, HasXProperty))

class HasGetAttrAndSetAttr:
    def __getattr__(self, attr: str) -> MyInt:
        return MyInt(0)

    def __setattr__(self, attr: str, value: int) -> None: ...

# TODO: these should pass
static_assert(is_subtype_of(HasGetAttrAndSetAttr, HasXProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasGetAttrAndSetAttr, HasXProperty))  # error: [static-assert-error]
static_assert(is_subtype_of(HasGetAttrAndSetAttr, XAsymmetricProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasGetAttrAndSetAttr, XAsymmetricProperty))  # error: [static-assert-error]
```

## Narrowing of protocols

By default, a protocol class cannot be used as the second argument to `isinstance()` or
`issubclass()`, and a type checker must emit an error on such calls. However, we still narrow the
type inside these branches (this matches the behaviour of other type checkers):

```py
from typing import Protocol

class HasX(Protocol):
    x: int

def f(arg: object, arg2: type):
    if isinstance(arg, HasX):  # error
        reveal_type(arg)  # revealed: HasX
    else:
        reveal_type(arg)  # revealed: ~HasX

    if issubclass(arg2, HasX):  # error
        reveal_type(arg2)  # revealed: type[HasX]
    else:
        reveal_type(arg2)  # revealed: type & ~type[HasX]
```

A protocol class decorated with `@typing(_extensions).runtime_checkable` *can* be used as the second
argument to `isisinstance()` at runtime:

```py
from typing import runtime_checkable

@runtime_checkable
class RuntimeCheckableHasX(Protocol):
    x: int

def f(arg: object):
    if isinstance(arg, RuntimeCheckableHasX):  # no error!
        reveal_type(arg)  # revealed: RuntimeCheckableHasX
    else:
        reveal_type(arg)  # revealed: ~RuntimeCheckableHasX
```

but in order for a protocol class to be used as the second argument to `issubclass()`, it must
satisfy two conditions:

1. It must be decorated with `@runtime_checkable`
1. It must *only* have method members (protocols with attribute members are not permitted)

```py
@runtime_checkable
class OnlyMethodMembers(Protocol):
    def method(self) -> None: ...

def f(arg1: type, arg2: type):
    if issubclass(arg1, OnlyMethodMembers):  # error
        reveal_type(arg1)  # revealed: type[OnlyMethodMembers]
    else:
        reveal_type(arg1)  # revealed: type & ~type[OnlyMethodMembers]

    if issubclass(arg2, OnlyMethodMembers):  # no error!
        reveal_type(arg2)  # revealed: type[OnlyMethodMembers]
    else:
        reveal_type(arg2)  # revealed: type & ~type[OnlyMethodMembers]
```

## `typing.SupportsIndex` and `typing.Sized`

`typing.SupportsIndex` is already somewhat supported through some special-casing in red-knot.

```py
from typing import SupportsIndex, Literal

def _(some_int: int, some_literal_int: Literal[1], some_indexable: SupportsIndex):
    a: SupportsIndex = some_int
    b: SupportsIndex = some_literal_int
    c: SupportsIndex = some_indexable
```

The same goes for `typing.Sized`:

```py
from typing import Sized

def _(some_list: list, some_tuple: tuple[int, str], some_sized: Sized):
    a: Sized = some_list
    b: Sized = some_tuple
    c: Sized = some_sized
```

## TODO

Add tests for:

- Assignments without declarations in protocol class bodies. And various weird ways of creating
    attributes in a class body or instance method. [Example mypy tests][mypy_weird_protocols].
- More tests for protocols inside `type[]`. [Spec reference][protocols_inside_type_spec].
- Protocols with instance-method members
- Protocols with `@classmethod` and `@staticmethod`
- Assignability of non-instance types to protocols with instance-method members (e.g. a
    class-literal type can be a subtype of `Sized` if its metaclass has a `__len__` method)
- Protocols with methods that have annotated `self` parameters.
    [Spec reference][self_types_protocols_spec].
- Protocols with overloaded method members
- `super()` on nominal subtypes (explicit and implicit) of protocol classes
- [Recursive protocols][recursive_protocols_spec]
- Generic protocols
- Non-generic protocols with function-scoped generic methods
- Protocols with instance attributes annotated with `Callable` (can a nominal type with a method
    satisfy that protocol, and if so in what cases?)
- Protocols decorated with `@final`
- Protocols with attribute members annotated with `Any`
- Protocols with methods that have parameters or the return type unannotated
- Protocols with methods that have parameters or the return type annotated with `Any`
- Equivalence and subtyping between `Callable` types and protocols that define `__call__`

[mypy_protocol_docs]: https://mypy.readthedocs.io/en/stable/protocols.html#protocols-and-structural-subtyping
[mypy_protocol_tests]: https://github.com/python/mypy/blob/master/test-data/unit/check-protocols.test
[mypy_weird_protocols]: https://github.com/python/mypy/blob/a3ce6d5307e99a1b6c181eaa7c5cf134c53b7d8b/test-data/unit/check-protocols.test#L2131-L2132
[protocol conformance tests]: https://github.com/python/typing/tree/main/conformance/tests
[protocols_inside_type_spec]: https://typing.python.org/en/latest/spec/protocol.html#type-and-class-objects-vs-protocols
[recursive_protocols_spec]: https://typing.python.org/en/latest/spec/protocol.html#recursive-protocols
[self_types_protocols_spec]: https://typing.python.org/en/latest/spec/protocol.html#self-types-in-protocols
[typing_spec_protocols]: https://typing.python.org/en/latest/spec/protocol.html
