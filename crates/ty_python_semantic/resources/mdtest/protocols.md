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
types, on the other hand: a type which is defined by its properties and behavior.

## Defining a protocol

```toml
[environment]
python-version = "3.12"
```

A protocol is defined by inheriting from the `Protocol` class, which is annotated as an instance of
`_SpecialForm` in typeshed's stubs.

```py
from typing import Protocol
from ty_extensions import reveal_mro

class MyProtocol(Protocol): ...

reveal_mro(MyProtocol)  # revealed: (<class 'MyProtocol'>, typing.Protocol, typing.Generic, <class 'object'>)
```

Just like for any other class base, it is an error for `Protocol` to appear multiple times in a
class's bases:

```py
class Foo(Protocol, Protocol): ...  # error: [duplicate-base]

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)
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

# error: [invalid-generic-class] "Cannot both inherit from subscripted `Protocol` and use PEP 695 type variables"
class Bar3[T](Protocol[T]):
    x: T

# Note that this class definition *will* actually succeed at runtime,
# unlike classes that combine PEP-695 type parameters with inheritance from `Generic[]`
reveal_mro(Bar3)  # revealed: (<class 'Bar3[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
```

It's an error to include both bare `Protocol` and subscripted `Protocol[]` in the bases list
simultaneously:

```py
class DuplicateBases(Protocol, Protocol[T]):  # error: [duplicate-base]
    x: T

# revealed: (<class 'DuplicateBases[Unknown]'>, Unknown, <class 'object'>)
reveal_mro(DuplicateBases)
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

Note, however, that `is_protocol` returns `False` at runtime for specializations of generic
protocols. We still consider these to be "protocol classes" internally, regardless:

```py
class MyGenericProtocol[T](Protocol):
    x: T

reveal_type(is_protocol(MyGenericProtocol))  # revealed: Literal[True]

# We still consider this a protocol class internally,
# but the inferred type of the call here reflects the result at runtime:
reveal_type(is_protocol(MyGenericProtocol[int]))  # revealed: Literal[False]
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

# revealed: (<class 'SubclassOfMyProtocol'>, <class 'MyProtocol'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(SubclassOfMyProtocol)

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

# revealed: (<class 'ComplexInheritance'>, <class 'SubProtocol'>, <class 'MyProtocol'>, <class 'OtherProtocol'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(ComplexInheritance)

reveal_type(is_protocol(ComplexInheritance))  # revealed: Literal[True]
```

If `Protocol` is present in the bases tuple, all other bases in the tuple must be protocol classes,
or `TypeError` is raised at runtime when the class is created.

```py
# error: [invalid-protocol] "Protocol class `Invalid` cannot inherit from non-protocol class `NotAProtocol`"
class Invalid(NotAProtocol, Protocol): ...

# revealed: (<class 'Invalid'>, <class 'NotAProtocol'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Invalid)

# error: [invalid-protocol] "Protocol class `AlsoInvalid` cannot inherit from non-protocol class `NotAProtocol`"
class AlsoInvalid(MyProtocol, OtherProtocol, NotAProtocol, Protocol): ...

# revealed: (<class 'AlsoInvalid'>, <class 'MyProtocol'>, <class 'OtherProtocol'>, <class 'NotAProtocol'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(AlsoInvalid)

class NotAGenericProtocol[T]: ...

# error: [invalid-protocol] "Protocol class `StillInvalid` cannot inherit from non-protocol class `NotAGenericProtocol`"
class StillInvalid(NotAGenericProtocol[int], Protocol): ...

# revealed: (<class 'StillInvalid'>, <class 'NotAGenericProtocol[int]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(StillInvalid)
```

But two exceptions to this rule are `object` and `Generic`:

```py
from typing import TypeVar, Generic

T = TypeVar("T")

# Note: pyright and pyrefly do not consider this to be a valid `Protocol` class,
# but mypy does (and has an explicit test for this behavior). Mypy was the
# reference implementation for PEP-544, and its behavior also matches the CPython
# runtime, so we choose to follow its behavior here rather than that of the other
# type checkers.
class Fine(Protocol, object): ...

reveal_mro(Fine)  # revealed: (<class 'Fine'>, typing.Protocol, typing.Generic, <class 'object'>)

class StillFine(Protocol, Generic[T], object): ...
class EvenThis[T](Protocol, object): ...
class OrThis(Protocol[T], Generic[T]): ...
class AndThis(Protocol[T], Generic[T], object): ...
```

And multiple inheritance from a mix of protocol and non-protocol classes is fine as long as
`Protocol` itself is not in the bases list:

```py
class FineAndDandy(MyProtocol, OtherProtocol, NotAProtocol): ...

# revealed: (<class 'FineAndDandy'>, <class 'MyProtocol'>, <class 'OtherProtocol'>, typing.Protocol, typing.Generic, <class 'NotAProtocol'>, <class 'object'>)
reveal_mro(FineAndDandy)
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
from ty_extensions import static_assert, is_subtype_of, is_assignable_to

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
    y: type[Protocol],  # error: [invalid-type-form] "`typing.Protocol` is not allowed in type expressions"
):
    reveal_type(x)  # revealed: Unknown
    reveal_type(y)  # revealed: type[Unknown]

# fmt: on
```

Nonetheless, `Protocol` is an instance of `type` at runtime, and therefore can still be used as the
second argument to `issubclass()` at runtime:

```py
import abc
import typing
from ty_extensions import TypeOf, reveal_mro

reveal_type(type(Protocol))  # revealed: <class '_ProtocolMeta'>
# revealed: (<class '_ProtocolMeta'>, <class 'ABCMeta'>, <class 'type'>, <class 'object'>)
reveal_mro(type(Protocol))
static_assert(is_subtype_of(TypeOf[Protocol], type))
static_assert(is_subtype_of(TypeOf[Protocol], abc.ABCMeta))
static_assert(is_subtype_of(TypeOf[Protocol], typing._ProtocolMeta))

# Could also be `Literal[True]`, but `bool` is fine:
reveal_type(issubclass(MyProtocol, Protocol))  # revealed: bool
```

## `typing.Protocol` versus `typing_extensions.Protocol`

`typing.Protocol` and its backport in `typing_extensions` should be treated as exactly equivalent.

```py
import typing
import typing_extensions
from ty_extensions import static_assert, is_equivalent_to, TypeOf

static_assert(is_equivalent_to(TypeOf[typing.Protocol], TypeOf[typing_extensions.Protocol]))
static_assert(is_equivalent_to(int | str | TypeOf[typing.Protocol], TypeOf[typing_extensions.Protocol] | str | int))

class Foo(typing.Protocol):
    x: int

class Bar(typing_extensions.Protocol):
    x: int

static_assert(typing_extensions.is_protocol(Foo))
static_assert(typing_extensions.is_protocol(Bar))
static_assert(is_equivalent_to(Foo, Bar))
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
static_assert(is_equivalent_to(RuntimeCheckableFoo, RuntimeCheckableBar))

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

<!-- snapshot-diagnostics -->

Neither `Protocol`, nor any protocol class, can be directly instantiated:

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import Protocol

# error: [call-non-callable]
reveal_type(Protocol())  # revealed: Unknown

class MyProtocol(Protocol):
    x: int

# error: [call-non-callable] "Cannot instantiate class `MyProtocol`"
reveal_type(MyProtocol())  # revealed: MyProtocol

class GenericProtocol[T](Protocol):
    x: T

# error: [call-non-callable] "Cannot instantiate class `GenericProtocol`"
reveal_type(GenericProtocol[int]())  # revealed: GenericProtocol[int]
```

But a non-protocol class can be instantiated, even if it has `Protocol` in its MRO:

```py
class SubclassOfMyProtocol(MyProtocol): ...

reveal_type(SubclassOfMyProtocol())  # revealed: SubclassOfMyProtocol

class SubclassOfGenericProtocol[T](GenericProtocol[T]): ...

reveal_type(SubclassOfGenericProtocol[int]())  # revealed: SubclassOfGenericProtocol[int]
```

And as a corollary, `type[MyProtocol]` can also be called:

```py
def f(x: type[MyProtocol]):
    reveal_type(x())  # revealed: @Todo(type[T] for protocols)
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

reveal_type(get_protocol_members(Foo))  # revealed: frozenset[Literal["method_member", "x", "y", "z"]]
```

To see the kinds and types of the protocol members, you can use the debugging aid
`ty_extensions.reveal_protocol_interface`, meanwhile:

```py
from ty_extensions import reveal_protocol_interface
from typing import SupportsIndex, SupportsAbs, ClassVar, Iterator

# revealed: {"method_member": MethodMember(`(self, /) -> bytes`), "x": AttributeMember(`int`), "y": PropertyMember { getter: `def y(self, /) -> str` }, "z": PropertyMember { getter: `def z(self, /) -> int`, setter: `def z(self, /, z: int) -> None` }}
reveal_protocol_interface(Foo)
# revealed: {"__index__": MethodMember(`(self, /) -> int`)}
reveal_protocol_interface(SupportsIndex)
# revealed: {"__abs__": MethodMember(`(self, /) -> Unknown`)}
reveal_protocol_interface(SupportsAbs)
# revealed: {"__iter__": MethodMember(`(self, /) -> Iterator[Unknown]`), "__next__": MethodMember(`(self, /) -> Unknown`)}
reveal_protocol_interface(Iterator)

# error: [invalid-argument-type] "Invalid argument to `reveal_protocol_interface`: Only protocol classes can be passed to `reveal_protocol_interface`"
reveal_protocol_interface(int)
# error: [invalid-argument-type] "Argument to function `reveal_protocol_interface` is incorrect: Expected `type`, found `Literal["foo"]`"
reveal_protocol_interface("foo")
```

Similar to the way that `typing.is_protocol` returns `False` at runtime for all generic aliases,
`typing.get_protocol_members` raises an exception at runtime if you pass it a generic alias, so we
do not implement any special handling for generic aliases passed to the function.
`ty_extensions.reveal_protocol_interface` can be used on both, however:

```py
# TODO: these fail at runtime, but we don't emit `[invalid-argument-type]` diagnostics
# currently due to https://github.com/astral-sh/ty/issues/116
reveal_type(get_protocol_members(SupportsAbs[int]))  # revealed: frozenset[str]
reveal_type(get_protocol_members(Iterator[int]))  # revealed: frozenset[str]

# revealed: {"__abs__": MethodMember(`(self, /) -> int`)}
reveal_protocol_interface(SupportsAbs[int])
# revealed: {"__iter__": MethodMember(`(self, /) -> Iterator[int]`), "__next__": MethodMember(`(self, /) -> int`)}
reveal_protocol_interface(Iterator[int])

class BaseProto(Protocol):
    def member(self) -> int: ...

class SubProto(BaseProto, Protocol):
    def member(self) -> bool: ...

# revealed: {"member": MethodMember(`(self, /) -> int`)}
reveal_protocol_interface(BaseProto)

# revealed: {"member": MethodMember(`(self, /) -> bool`)}
reveal_protocol_interface(SubProto)

class ProtoWithClassVar(Protocol):
    x: ClassVar[int]

# revealed: {"x": AttributeMember(`int`; ClassVar)}
reveal_protocol_interface(ProtoWithClassVar)

class ProtocolWithDefault(Protocol):
    x: int = 0

# We used to incorrectly report this as having an `x: Literal[0]` member;
# declared types should take priority over inferred types for protocol interfaces!
#
# revealed: {"x": AttributeMember(`int`)}
reveal_protocol_interface(ProtocolWithDefault)
```

Certain special attributes and methods are not considered protocol members at runtime, and should
not be considered protocol members by type checkers either:

```py
class Lumberjack(Protocol):
    __slots__ = ()
    __match_args__ = ()
    _abc_foo: str  # any attribute starting with `_abc_` is excluded as a protocol attribute
    x: int

    def __new__(cls, x: int) -> "Lumberjack":
        return object.__new__(cls)

    def __init__(self, x: int) -> None:
        self.x = x

reveal_type(get_protocol_members(Lumberjack))  # revealed: frozenset[Literal["x"]]
```

A sub-protocol inherits and extends the members of its superclass protocol(s):

```py
class Bar(Protocol):
    spam: str

class Baz(Bar, Protocol):
    ham: memoryview

reveal_type(get_protocol_members(Baz))  # revealed: frozenset[Literal["ham", "spam"]]

class Baz2(Bar, Foo, Protocol): ...

# revealed: frozenset[Literal["method_member", "spam", "x", "y", "z"]]
reveal_type(get_protocol_members(Baz2))
```

## Protocol members in statically known branches

<!-- snapshot-diagnostics -->

The list of protocol members does not include any members declared in branches that are statically
known to be unreachable:

```toml
[environment]
python-version = "3.9"
```

```py
import sys
from typing_extensions import Protocol, get_protocol_members

class Foo(Protocol):
    if sys.version_info >= (3, 10):
        a: int
        b = 42
        def c(self) -> None: ...
    else:
        d: int
        e = 56  # error: [ambiguous-protocol-member]
        def f(self) -> None: ...

reveal_type(get_protocol_members(Foo))  # revealed: frozenset[Literal["d", "e", "f"]]
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

The original class object must be passed to the function; a specialized version of a generic version
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
from typing import Protocol, Any, ClassVar
from collections.abc import Sequence
from ty_extensions import static_assert, is_assignable_to, is_subtype_of

class HasX(Protocol):
    x: int

class HasXY(Protocol):
    x: int
    y: int

class Foo:
    x: int

class IntSub(int): ...

class HasXIntSub(Protocol):
    x: IntSub

static_assert(is_subtype_of(Foo, HasX))
static_assert(is_assignable_to(Foo, HasX))
static_assert(not is_subtype_of(Foo, HasXY))
static_assert(not is_assignable_to(Foo, HasXY))

static_assert(not is_subtype_of(HasXIntSub, HasX))
static_assert(not is_assignable_to(HasXIntSub, HasX))
static_assert(not is_subtype_of(HasX, HasXIntSub))
static_assert(not is_assignable_to(HasX, HasXIntSub))

class FooSub(Foo): ...

static_assert(is_subtype_of(FooSub, HasX))
static_assert(is_assignable_to(FooSub, HasX))
static_assert(not is_subtype_of(FooSub, HasXY))
static_assert(not is_assignable_to(FooSub, HasXY))

class FooBool:
    x: bool

static_assert(not is_subtype_of(FooBool, HasX))
static_assert(not is_assignable_to(FooBool, HasX))

class FooAny:
    x: Any

static_assert(not is_subtype_of(FooAny, HasX))
static_assert(is_assignable_to(FooAny, HasX))

class SubclassOfAny(Any): ...

class FooSubclassOfAny:
    x: SubclassOfAny

static_assert(not is_subtype_of(FooSubclassOfAny, HasX))

# `FooSubclassOfAny` is assignable to `HasX` for the following reason. The `x` attribute on `FooSubclassOfAny`
# is accessible on the class itself. When accessing `x` on an instance, the descriptor protocol is invoked, and
# `__get__` is looked up on `SubclassOfAny`. Every member access on `SubclassOfAny` yields `Any`, so `__get__` is
# also available, and calling `Any` also yields `Any`. Thus, accessing `x` on an instance of `FooSubclassOfAny`
# yields `Any`, which is assignable to `int` and vice versa.
static_assert(is_assignable_to(FooSubclassOfAny, HasX))

class FooWithY(Foo):
    y: int

assert is_subtype_of(FooWithY, HasXY)
static_assert(is_assignable_to(FooWithY, HasXY))

class Bar:
    x: str

static_assert(not is_subtype_of(Bar, HasX))
static_assert(not is_assignable_to(Bar, HasX))

class Baz:
    y: int

static_assert(not is_subtype_of(Baz, HasX))
static_assert(not is_assignable_to(Baz, HasX))

class Qux:
    def __init__(self, x: int) -> None:
        self.x: int = x

static_assert(is_subtype_of(Qux, HasX))
static_assert(is_assignable_to(Qux, HasX))

class HalfUnknownQux:
    def __init__(self, x: int) -> None:
        self.x = x

reveal_type(HalfUnknownQux(1).x)  # revealed: Unknown | int

static_assert(not is_subtype_of(HalfUnknownQux, HasX))
static_assert(is_assignable_to(HalfUnknownQux, HasX))

class FullyUnknownQux:
    def __init__(self, x) -> None:
        self.x = x

static_assert(not is_subtype_of(FullyUnknownQux, HasX))
static_assert(is_assignable_to(FullyUnknownQux, HasX))

class HasXWithDefault(Protocol):
    x: int = 0

class FooWithZero:
    x: int = 0

static_assert(is_subtype_of(FooWithZero, HasXWithDefault))
static_assert(is_assignable_to(FooWithZero, HasXWithDefault))

# TODO: whether or not any of these four assertions should pass is not clearly specified.
#
# A test in the typing conformance suite implies that they all should:
# that a nominal class with an instance attribute `x`
# (*without* a default value on the class body)
# should be understood as satisfying a protocol that has an attribute member `x`
# even if the protocol's `x` member has a default value on the class body.
#
# See <https://github.com/python/typing/blob/d4f39b27a4a47aac8b6d4019e1b0b5b3156fabdc/conformance/tests/protocols_definition.py#L56-L79>.
#
# The implications of this for meta-protocols are not clearly spelled out, however,
# and the fact that attribute members on protocols can have defaults is only mentioned
# in a throwaway comment in the spec's prose.
static_assert(is_subtype_of(Foo, HasXWithDefault))
static_assert(is_assignable_to(Foo, HasXWithDefault))
static_assert(is_subtype_of(Qux, HasXWithDefault))
static_assert(is_assignable_to(Qux, HasXWithDefault))

class HasClassVarX(Protocol):
    x: ClassVar[int]

static_assert(is_subtype_of(FooWithZero, HasClassVarX))
static_assert(is_assignable_to(FooWithZero, HasClassVarX))
# TODO: these should pass
static_assert(not is_subtype_of(Foo, HasClassVarX))  # error: [static-assert-error]
static_assert(not is_assignable_to(Foo, HasClassVarX))  # error: [static-assert-error]
static_assert(not is_subtype_of(Qux, HasClassVarX))  # error: [static-assert-error]
static_assert(not is_assignable_to(Qux, HasClassVarX))  # error: [static-assert-error]

static_assert(is_subtype_of(Sequence[Foo], Sequence[HasX]))
static_assert(is_assignable_to(Sequence[Foo], Sequence[HasX]))
static_assert(not is_subtype_of(list[Foo], list[HasX]))
static_assert(not is_assignable_to(list[Foo], list[HasX]))
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

# TODO: these should pass
static_assert(not is_subtype_of(A, HasX))  # error: [static-assert-error]
static_assert(not is_assignable_to(A, HasX))  # error: [static-assert-error]

class B:
    x: Final = 42

# TODO: these should pass
static_assert(not is_subtype_of(A, HasX))  # error: [static-assert-error]
static_assert(not is_assignable_to(A, HasX))  # error: [static-assert-error]

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

static_assert(is_subtype_of(MutableDataclass, HasX))
static_assert(is_assignable_to(MutableDataclass, HasX))

@dataclass(frozen=True)
class ImmutableDataclass:
    x: int

# TODO: these should pass
static_assert(not is_subtype_of(ImmutableDataclass, HasX))  # error: [static-assert-error]
static_assert(not is_assignable_to(ImmutableDataclass, HasX))  # error: [static-assert-error]

class NamedTupleWithX(NamedTuple):
    x: int

# TODO: these should pass
static_assert(not is_subtype_of(NamedTupleWithX, HasX))  # error: [static-assert-error]
static_assert(not is_assignable_to(NamedTupleWithX, HasX))  # error: [static-assert-error]
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

static_assert(is_subtype_of(XProperty, HasX))
static_assert(is_assignable_to(XProperty, HasX))
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

Assignments in a class body of a protocol -- of any kind -- are not permitted by ty unless the
symbol being assigned to is also explicitly declared in the body of the protocol class or one of its
superclasses. Note that this is stricter validation of protocol members than many other type
checkers currently apply (as of 2025/04/21).

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
    e = 72  # error: [ambiguous-protocol-member]

    # error: [ambiguous-protocol-member] "Consider adding an annotation, e.g. `f: int = ...`"
    # error: [ambiguous-protocol-member] "Consider adding an annotation, e.g. `g: int = ...`"
    f, g = (1, 2)

    h: int = (i := 3)  # error: [ambiguous-protocol-member]

    for j in range(42):  # error: [ambiguous-protocol-member]
        pass

    with MyContext() as k:  # error: [ambiguous-protocol-member]
        pass

    match object():
        case l:  # error: [ambiguous-protocol-member]
            ...
    # error: [ambiguous-protocol-member] "Consider adding an annotation, e.g. `m: int | str = ...`"
    m = 1 if 1.2 > 3.4 else "a"

# revealed: frozenset[Literal["Nested", "NestedProtocol", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m"]]
reveal_type(get_protocol_members(LotsOfBindings))

class Foo(Protocol):
    a: int

class Bar(Foo, Protocol):
    a = 42  # fine, because it's declared in the superclass

reveal_type(get_protocol_members(Bar))  # revealed: frozenset[Literal["a"]]
```

A binding-without-declaration will not be reported if it occurs in a branch that we can statically
determine to be unreachable. The reason is that we don't consider it to be a protocol member at all
if all definitions for the variable are in unreachable blocks:

```py
import sys

class Protocol694(Protocol):
    if sys.version_info > (3, 694):
        x = 42  # no error!
```

If there are multiple bindings of the variable in the class body, however, and at least one of the
bindings occurs in a block of code that is understood to be (possibly) reachable, a diagnostic will
be reported. The diagnostic will be attached to the first binding that occurs in the class body,
even if that first definition occurs in an unreachable block:

```py
class Protocol695(Protocol):
    if sys.version_info > (3, 695):
        x = 42
    else:
        x = 42

    x = 56  # error: [ambiguous-protocol-member]
```

In order for the variable to be considered declared, the declaration of the variable must also take
place in a block of code that is understood to be (possibly) reachable:

```py
class Protocol696(Protocol):
    if sys.version_info > (3, 696):
        x: int
    else:
        x = 42  # error: [ambiguous-protocol-member]
        y: int

    y = 56  # no error
```

Attribute members are allowed to have assignments in methods on the protocol class, just like
non-protocol classes. Unlike other classes, however, instance attributes that are not declared in
the class body are disallowed. This is mandated by [the spec][spec_protocol_members]:

> Additional attributes *only* defined in the body of a method by assignment via `self` are not
> allowed. The rationale for this is that the protocol class implementation is often not shared by
> subtypes, so the interface should not depend on the default implementation.

```py
class Foo(Protocol):
    x: int
    y: str

    def __init__(self) -> None:
        self.x = 42  # fine

        self.a = 56  # TODO: should emit diagnostic
        self.b: int = 128  # TODO: should emit diagnostic

    def non_init_method(self) -> None:
        self.x = 64  # fine
        self.y = "bar"  # fine

        self.c = 72  # TODO: should emit diagnostic

# Note: the list of members does not include `a`, `b` or `c`,
# as none of these attributes is declared in the class body.
reveal_type(get_protocol_members(Foo))  # revealed: frozenset[Literal["non_init_method", "x", "y"]]
```

If a member is declared in a superclass of a protocol class, it is fine for it to be assigned to in
the sub-protocol class without a redeclaration:

```py
class Super(Protocol):
    x: int

class Sub(Super, Protocol):
    x = 42  # no error here, since it's declared in the superclass

reveal_type(get_protocol_members(Super))  # revealed: frozenset[Literal["x"]]
reveal_type(get_protocol_members(Sub))  # revealed: frozenset[Literal["x"]]
```

If a protocol has 0 members, then all other types are assignable to it, and all fully static types
are subtypes of it:

```py
from typing import Protocol

class UniversalSet(Protocol): ...

static_assert(is_assignable_to(object, UniversalSet))
static_assert(is_subtype_of(object, UniversalSet))
```

Which means that `UniversalSet` here is in fact an equivalent type to `object`:

```py
from ty_extensions import is_equivalent_to

static_assert(is_equivalent_to(UniversalSet, object))
```

and that therefore `Any` is a subtype of `UniversalSet` (in general, `Any` can *only* ever be a
subtype of `object` and types that are equivalent to `object`):

```py
static_assert(is_subtype_of(Any, UniversalSet))
```

`object` is a subtype of certain other protocols too. Since all fully static types (whether nominal
or structural) are subtypes of `object`, these protocols are also subtypes of `object`; and this
means that these protocols are also equivalent to `UniversalSet` and `object`:

```py
class SupportsStr(Protocol):
    def __str__(self) -> str: ...

static_assert(is_equivalent_to(SupportsStr, UniversalSet))
static_assert(is_equivalent_to(SupportsStr, object))
static_assert(is_subtype_of(SupportsStr, UniversalSet))
static_assert(is_subtype_of(UniversalSet, SupportsStr))
static_assert(is_assignable_to(UniversalSet, SupportsStr))
static_assert(is_assignable_to(SupportsStr, UniversalSet))

class SupportsClass(Protocol):
    @property
    def __class__(self) -> type: ...

static_assert(is_equivalent_to(SupportsClass, UniversalSet))
static_assert(is_equivalent_to(SupportsClass, SupportsStr))
static_assert(is_equivalent_to(SupportsClass, object))

static_assert(is_subtype_of(SupportsClass, SupportsStr))
static_assert(is_subtype_of(SupportsStr, SupportsClass))
static_assert(is_assignable_to(SupportsStr, SupportsClass))
static_assert(is_assignable_to(SupportsClass, SupportsStr))
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

Since `object` defines a `__hash__` method, this means that the standard-library `Hashable` protocol
is currently understood by ty as being equivalent to `object`, much like `SupportsStr` and
`UniversalSet` above:

```py
from typing import Hashable

static_assert(is_equivalent_to(object, Hashable))
static_assert(is_assignable_to(object, Hashable))
static_assert(is_subtype_of(object, Hashable))
```

This means that any type considered assignable to `object` (which is all types) is considered by ty
to be assignable to `Hashable`. This avoids false positives on code like this:

```py
from typing import Sequence
from ty_extensions import is_disjoint_from

def takes_hashable_or_sequence(x: Hashable | list[Hashable]): ...

takes_hashable_or_sequence(["foo"])  # fine
takes_hashable_or_sequence(None)  # fine

static_assert(not is_disjoint_from(list[str], Hashable | list[Hashable]))
static_assert(not is_disjoint_from(list[str], Sequence[Hashable]))

static_assert(is_subtype_of(list[Hashable], Sequence[Hashable]))
static_assert(is_subtype_of(list[str], Sequence[Hashable]))
```

but means that ty currently does not detect errors on code like this, which is flagged by other type
checkers:

```py
def needs_something_hashable(x: Hashable):
    hash(x)

needs_something_hashable([])
```

## Diagnostics for protocols with invalid attribute members

This is a short appendix to the previous section with the `snapshot-diagnostics` directive enabled
(enabling snapshots for the previous section in its entirety would lead to a huge snapshot, since
it's a large section).

<!-- snapshot-diagnostics -->

`a.py`:

```py
from typing import Protocol

def coinflip() -> bool:
    return True

class A(Protocol):
    # The `x` and `y` members attempt to use Python-2-style type comments
    # to indicate that the type should be `int | None` and `str` respectively,
    # but we don't support those

    # error: [ambiguous-protocol-member]
    a = None  # type: int
    # error: [ambiguous-protocol-member]
    b = ...  # type: str

    if coinflip():
        c = 1  # error: [ambiguous-protocol-member]
    else:
        c = 2

    # error: [ambiguous-protocol-member]
    for d in range(42):
        pass
```

Validation of protocols that had cross-module inheritance used to break, so we test that explicitly
here too:

`b.py`:

```py
from typing import Protocol

# Ensure the number of scopes in `b.py` is greater than the number of scopes in `c.py`:
class SomethingUnrelated: ...

class A(Protocol):
    x: int
```

`c.py`:

```py
from b import A
from typing import Protocol

class C(A, Protocol):
    x = 42  # fine, due to declaration in the base class
```

## Equivalence of protocols

```toml
[environment]
python-version = "3.12"
```

Two protocols are considered equivalent types if they specify the same interface, even if they have
different names:

```py
from typing import Protocol
from ty_extensions import is_equivalent_to, static_assert

class HasX(Protocol):
    x: int

class AlsoHasX(Protocol):
    x: int

static_assert(is_equivalent_to(HasX, AlsoHasX))
```

And unions containing equivalent protocols are recognized as equivalent, even when the order is not
identical:

```py
class HasY(Protocol):
    y: str

class AlsoHasY(Protocol):
    y: str

class A: ...
class B: ...

static_assert(is_equivalent_to(A | HasX | B | HasY, B | AlsoHasY | AlsoHasX | A))
```

Protocols are considered equivalent if their members are equivalent, even if those members are
differently ordered unions:

```py
class C: ...

class UnionProto1(Protocol):
    x: A | B | C

class UnionProto2(Protocol):
    x: C | A | B

static_assert(is_equivalent_to(UnionProto1, UnionProto2))
static_assert(is_equivalent_to(UnionProto1 | A | B, B | UnionProto2 | A))
```

Different generic protocols with equivalent specializations can be equivalent, but generic protocols
with different specializations are not considered equivalent:

```py
from typing import TypeVar

S = TypeVar("S")

class NonGenericProto1(Protocol):
    x: int
    y: str

class NonGenericProto2(Protocol):
    y: str
    x: int

class Nominal1: ...
class Nominal2: ...

class GenericProto[T](Protocol):
    x: T

class LegacyGenericProto(Protocol[S]):
    x: S

static_assert(is_equivalent_to(GenericProto[int], LegacyGenericProto[int]))
static_assert(is_equivalent_to(GenericProto[NonGenericProto1], LegacyGenericProto[NonGenericProto2]))

static_assert(
    is_equivalent_to(
        GenericProto[NonGenericProto1 | Nominal1 | Nominal2], LegacyGenericProto[Nominal2 | Nominal1 | NonGenericProto2]
    )
)

static_assert(not is_equivalent_to(GenericProto[str], GenericProto[int]))
static_assert(not is_equivalent_to(GenericProto[str], LegacyGenericProto[int]))
static_assert(not is_equivalent_to(GenericProto, GenericProto[int]))
static_assert(not is_equivalent_to(LegacyGenericProto, LegacyGenericProto[int]))
```

## Intersections of protocols

An intersection of two protocol types `X` and `Y` is equivalent to a protocol type `Z` that inherits
from both `X` and `Y`:

```py
from typing import Protocol
from ty_extensions import Intersection, static_assert, is_equivalent_to

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
from ty_extensions import is_disjoint_from

class NotFinalNominal: ...

@final
class FinalNominal: ...

static_assert(not is_disjoint_from(NotFinalNominal, HasX))
static_assert(is_disjoint_from(FinalNominal, HasX))

def _(arg1: Intersection[HasX, NotFinalNominal], arg2: Intersection[HasX, FinalNominal]):
    reveal_type(arg1)  # revealed: HasX & NotFinalNominal
    reveal_type(arg2)  # revealed: Never
```

The disjointness of a single protocol member with the type of an attribute on another type is enough
to make the whole protocol disjoint from the other type, even if all other members on the protocol
are satisfied by the other type. This applies to both `@final` types and non-final types:

```py
class Proto(Protocol):
    x: int
    y: str
    z: bytes

class Foo:
    x: int
    y: str
    z: None

static_assert(is_disjoint_from(Proto, Foo))

@final
class FinalFoo:
    x: int
    y: str
    z: None

static_assert(is_disjoint_from(Proto, FinalFoo))
```

## Intersections of protocols with types that have possibly unbound attributes

Note that if a `@final` class has a possibly unbound attribute corresponding to the protocol member,
instance types and class-literal types referring to that class cannot be a subtype of the protocol
but will also not be disjoint from the protocol:

`a.py`:

```py
from typing import final, ClassVar, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of, is_disjoint_from, is_assignable_to

def who_knows() -> bool:
    return False

@final
class Foo:
    if who_knows():
        x: ClassVar[int] = 42

class HasReadOnlyX(Protocol):
    @property
    def x(self) -> int: ...

static_assert(not is_subtype_of(Foo, HasReadOnlyX))
static_assert(not is_assignable_to(Foo, HasReadOnlyX))
static_assert(not is_disjoint_from(Foo, HasReadOnlyX))

static_assert(not is_subtype_of(type[Foo], HasReadOnlyX))
static_assert(not is_assignable_to(type[Foo], HasReadOnlyX))
static_assert(not is_disjoint_from(type[Foo], HasReadOnlyX))

static_assert(not is_subtype_of(TypeOf[Foo], HasReadOnlyX))
static_assert(not is_assignable_to(TypeOf[Foo], HasReadOnlyX))
static_assert(not is_disjoint_from(TypeOf[Foo], HasReadOnlyX))
```

A similar principle applies to module-literal types that have possibly unbound attributes:

`b.py`:

```py
def who_knows() -> bool:
    return False

if who_knows():
    x: int = 42
```

`c.py`:

```py
import b
from a import HasReadOnlyX
from ty_extensions import TypeOf, static_assert, is_subtype_of, is_disjoint_from, is_assignable_to

static_assert(not is_subtype_of(TypeOf[b], HasReadOnlyX))
static_assert(not is_assignable_to(TypeOf[b], HasReadOnlyX))
static_assert(not is_disjoint_from(TypeOf[b], HasReadOnlyX))
```

If the possibly unbound attribute's type is disjoint from the type of the protocol member, though,
it is still disjoint from the protocol. This applies to both `@final` types and non-final types:

`d.py`:

```py
from a import HasReadOnlyX, who_knows
from typing import final, ClassVar, Protocol
from ty_extensions import static_assert, is_disjoint_from, TypeOf

class Proto(Protocol):
    x: int

class Foo:
    def __init__(self):
        if who_knows():
            self.x: None = None

@final
class FinalFoo:
    def __init__(self):
        if who_knows():
            self.x: None = None

static_assert(is_disjoint_from(Foo, Proto))
static_assert(is_disjoint_from(FinalFoo, Proto))
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
from ty_extensions import is_subtype_of, is_assignable_to, static_assert, TypeOf

class HasX(Protocol):
    x: int

static_assert(is_subtype_of(TypeOf[module], HasX))
static_assert(is_assignable_to(TypeOf[module], HasX))

class ExplicitProtocolSubtype(HasX, Protocol):
    y: int

static_assert(is_subtype_of(ExplicitProtocolSubtype, HasX))
static_assert(is_assignable_to(ExplicitProtocolSubtype, HasX))

class ImplicitProtocolSubtype(Protocol):
    x: int
    y: str

static_assert(is_subtype_of(ImplicitProtocolSubtype, HasX))
static_assert(is_assignable_to(ImplicitProtocolSubtype, HasX))

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
from ty_extensions import is_subtype_of, is_assignable_to, static_assert

class ClassVarXProto(Protocol):
    x: ClassVar[int]

def f(obj: ClassVarXProto):
    reveal_type(obj.x)  # revealed: int
    reveal_type(type(obj).x)  # revealed: int
    obj.x = 42  # error: [invalid-attribute-access] "Cannot assign to ClassVar `x` from an instance of type `ClassVarXProto`"

class InstanceAttrX:
    x: int

# TODO: these should pass
static_assert(not is_assignable_to(InstanceAttrX, ClassVarXProto))  # error: [static-assert-error]
static_assert(not is_subtype_of(InstanceAttrX, ClassVarXProto))  # error: [static-assert-error]

class PropertyX:
    @property
    def x(self) -> int:
        return 42

# TODO: these should pass
static_assert(not is_assignable_to(PropertyX, ClassVarXProto))  # error: [static-assert-error]
static_assert(not is_subtype_of(PropertyX, ClassVarXProto))  # error: [static-assert-error]

class ClassVarX:
    x: ClassVar[int] = 42

static_assert(is_assignable_to(ClassVarX, ClassVarXProto))
static_assert(is_subtype_of(ClassVarX, ClassVarXProto))
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
from ty_extensions import is_subtype_of, is_assignable_to, static_assert

class HasXProperty(Protocol):
    @property
    def x(self) -> int: ...

class XAttr:
    x: int

static_assert(is_subtype_of(XAttr, HasXProperty))
static_assert(is_assignable_to(XAttr, HasXProperty))

class XReadProperty:
    @property
    def x(self) -> int:
        return 42

static_assert(is_subtype_of(XReadProperty, HasXProperty))
static_assert(is_assignable_to(XReadProperty, HasXProperty))

class XReadWriteProperty:
    @property
    def x(self) -> int:
        return 42

    @x.setter
    def x(self, val: int) -> None: ...

static_assert(is_subtype_of(XReadWriteProperty, HasXProperty))
static_assert(is_assignable_to(XReadWriteProperty, HasXProperty))

class XClassVar:
    x: ClassVar[int] = 42

static_assert(is_subtype_of(XClassVar, HasXProperty))
static_assert(is_assignable_to(XClassVar, HasXProperty))

class XFinal:
    x: Final[int] = 42

static_assert(is_subtype_of(XFinal, HasXProperty))
static_assert(is_assignable_to(XFinal, HasXProperty))

class XImplicitFinal:
    x: Final = 42

static_assert(is_subtype_of(XImplicitFinal, HasXProperty))
static_assert(is_assignable_to(XImplicitFinal, HasXProperty))
```

But only if it has the correct type:

```py
class XAttrBad:
    x: str

class HasStrXProperty(Protocol):
    @property
    def x(self) -> str: ...

# TODO: these should pass
static_assert(not is_assignable_to(XAttrBad, HasXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(HasStrXProperty, HasXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(HasXProperty, HasStrXProperty))  # error: [static-assert-error]
```

A read-only property on a protocol, unlike a mutable attribute, is covariant: `XSub` in the below
example satisfies the `HasXProperty` interface even though the type of the `x` attribute on `XSub`
is a subtype of `int` rather than being exactly `int`.

```py
class MyInt(int): ...

class XSub:
    x: MyInt

static_assert(is_subtype_of(XSub, HasXProperty))
static_assert(is_assignable_to(XSub, HasXProperty))

class XSubProto(Protocol):
    @property
    def x(self) -> XSub: ...

static_assert(is_subtype_of(XSubProto, HasXProperty))
static_assert(is_assignable_to(XSubProto, HasXProperty))
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

static_assert(is_subtype_of(XAttr, HasXProperty))
static_assert(is_assignable_to(XAttr, HasXProperty))

class XReadProperty:
    @property
    def x(self) -> int:
        return 42

# TODO: these should pass
static_assert(not is_subtype_of(XReadProperty, HasMutableXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(XReadProperty, HasMutableXProperty))  # error: [static-assert-error]

class XReadWriteProperty:
    @property
    def x(self) -> int:
        return 42

    @x.setter
    def x(self, val: int) -> None: ...

static_assert(is_subtype_of(XReadWriteProperty, HasMutableXProperty))
static_assert(is_assignable_to(XReadWriteProperty, HasMutableXProperty))

class XSub:
    x: MyInt

# TODO: these should pass
static_assert(not is_subtype_of(XSub, HasMutableXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(XSub, HasMutableXProperty))  # error: [static-assert-error]
```

A protocol with a read/write property `x` is exactly equivalent to a protocol with a mutable
attribute `x`. Both are subtypes of a protocol with a read-only property `x`:

```py
from ty_extensions import is_equivalent_to

class HasMutableXAttr(Protocol):
    x: int

# TODO: should pass
static_assert(is_equivalent_to(HasMutableXAttr, HasMutableXProperty))  # error: [static-assert-error]

static_assert(is_subtype_of(HasMutableXAttr, HasXProperty))
static_assert(is_assignable_to(HasMutableXAttr, HasXProperty))

static_assert(is_subtype_of(HasMutableXAttr, HasMutableXProperty))
static_assert(is_assignable_to(HasMutableXAttr, HasMutableXProperty))

static_assert(is_subtype_of(HasMutableXProperty, HasXProperty))
static_assert(is_assignable_to(HasMutableXProperty, HasXProperty))

static_assert(is_subtype_of(HasMutableXProperty, HasMutableXAttr))
static_assert(is_assignable_to(HasMutableXProperty, HasMutableXAttr))

class HasMutableXAttrWrongType(Protocol):
    x: str

# TODO: these should pass
static_assert(not is_assignable_to(HasMutableXAttrWrongType, HasXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(HasMutableXAttrWrongType, HasMutableXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(HasMutableXProperty, HasMutableXAttrWrongType))  # error: [static-assert-error]
```

A read/write property on a protocol, where the setter accepts a subtype of the type returned by the
getter, can be satisfied by a mutable attribute of any type bounded by the upper bound of the
getter-returned type and the lower bound of the setter-accepted type.

This follows from the principle that a type `X` can only be a subtype of a given protocol if the
`X`'s behavior is a superset of the behavior specified by the interface declared by the protocol. In
the below example, the behavior of an instance of `XAttr` is a superset of the behavior specified by
the protocol `HasAsymmetricXProperty`. The protocol specifies that reading an `x` attribute on the
instance must resolve to an instance of `int` or a subclass thereof, and `XAttr` satisfies this
requirement. The protocol also specifies that you must be able to assign instances of `MyInt` to the
`x` attribute, and again this is satisfied by `XAttr`: on instances of `XAttr`, you can assign *any*
instance of `int` to the `x` attribute, and thus by extension you can assign any instance of
`IntSub` to the `x` attribute, since any instance of `IntSub` is an instance of `int`:

```py
class HasAsymmetricXProperty(Protocol):
    @property
    def x(self) -> int: ...
    @x.setter
    def x(self, val: MyInt) -> None: ...

class XAttr:
    x: int

static_assert(is_subtype_of(XAttr, HasAsymmetricXProperty))
static_assert(is_assignable_to(XAttr, HasAsymmetricXProperty))
```

The end conclusion of this is that the getter-returned type of a property is always covariant and
the setter-accepted type is always contravariant. The combination of these leads to invariance for a
regular mutable attribute, where the implied getter-returned and setter-accepted types are the same.

```py
class XAttrSub:
    x: MyInt

static_assert(is_subtype_of(XAttrSub, HasAsymmetricXProperty))
static_assert(is_assignable_to(XAttrSub, HasAsymmetricXProperty))

class MyIntSub(MyInt):
    pass

class XAttrSubSub:
    x: MyIntSub

# TODO: should pass
static_assert(not is_subtype_of(XAttrSubSub, HasAsymmetricXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(XAttrSubSub, HasAsymmetricXProperty))  # error: [static-assert-error]
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

static_assert(is_subtype_of(XAsymmetricProperty, HasAsymmetricXProperty))
static_assert(is_assignable_to(XAsymmetricProperty, HasAsymmetricXProperty))
```

A custom descriptor attribute on the nominal class will also suffice:

```py
class Descriptor:
    def __get__(self, instance, owner) -> MyInt:
        return MyInt(0)

    def __set__(self, instance, value: int) -> None: ...

class XCustomDescriptor:
    x: Descriptor = Descriptor()

static_assert(is_subtype_of(XCustomDescriptor, HasAsymmetricXProperty))
static_assert(is_assignable_to(XCustomDescriptor, HasAsymmetricXProperty))
```

Moreover, a read-only property on a protocol can be satisfied by a nominal class that defines a
`__getattr__` method returning a suitable type. A read/write property can be satisfied by a nominal
class that defines a `__getattr__` method returning a suitable type *and* a `__setattr__` method
accepting a suitable type:

```py
class HasGetAttr:
    def __getattr__(self, attr: str) -> int:
        return 42

static_assert(is_subtype_of(HasGetAttr, HasXProperty))
static_assert(is_assignable_to(HasGetAttr, HasXProperty))

# TODO: these should pass
static_assert(not is_subtype_of(HasGetAttr, HasMutableXAttr))  # error: [static-assert-error]
static_assert(not is_subtype_of(HasGetAttr, HasMutableXAttr))  # error: [static-assert-error]

class HasGetAttrWithUnsuitableReturn:
    def __getattr__(self, attr: str) -> tuple[int, int]:
        return (1, 2)

# TODO: these should pass
static_assert(not is_subtype_of(HasGetAttrWithUnsuitableReturn, HasXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(HasGetAttrWithUnsuitableReturn, HasXProperty))  # error: [static-assert-error]

class HasGetAttrAndSetAttr:
    def __getattr__(self, attr: str) -> MyInt:
        return MyInt(0)

    def __setattr__(self, attr: str, value: int) -> None: ...

static_assert(is_subtype_of(HasGetAttrAndSetAttr, HasXProperty))
static_assert(is_assignable_to(HasGetAttrAndSetAttr, HasXProperty))

# TODO: these should pass
static_assert(is_subtype_of(HasGetAttrAndSetAttr, XAsymmetricProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasGetAttrAndSetAttr, XAsymmetricProperty))  # error: [static-assert-error]

class HasSetAttrWithUnsuitableInput:
    def __getattr__(self, attr: str) -> int:
        return 1

    def __setattr__(self, attr: str, value: str) -> None: ...

# TODO: these should pass
static_assert(not is_subtype_of(HasSetAttrWithUnsuitableInput, HasMutableXProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(HasSetAttrWithUnsuitableInput, HasMutableXProperty))  # error: [static-assert-error]
```

## Subtyping of protocols with method members

A protocol can have method members. `T` is assignable to `P` in the following example because the
class `T` has a method `m` which is assignable to the `Callable` supertype of the method `P.m`:

```py
from typing import Protocol
from ty_extensions import is_subtype_of, is_assignable_to, static_assert

class P(Protocol):
    def m(self, x: int, /) -> None: ...

class NominalSubtype:
    def m(self, y: int) -> None: ...

class NominalSubtype2:
    def m(self, *args: object) -> None: ...

class NotSubtype:
    def m(self, x: int) -> int:
        return 42

class NominalWithClassMethod:
    @classmethod
    def m(cls, x: int) -> None: ...

class NominalWithStaticMethod:
    @staticmethod
    def m(_, x: int) -> None: ...

class DefinitelyNotSubtype:
    m = None

static_assert(is_subtype_of(NominalSubtype, P))
static_assert(is_subtype_of(NominalSubtype2, P))
static_assert(is_subtype_of(NominalSubtype | NominalSubtype2, P))
static_assert(not is_assignable_to(DefinitelyNotSubtype, P))
static_assert(not is_assignable_to(NotSubtype, P))
static_assert(not is_assignable_to(NominalSubtype | NotSubtype, P))
static_assert(not is_assignable_to(NominalSubtype2 | DefinitelyNotSubtype, P))

# `m` has the correct signature when accessed on instances of `NominalWithClassMethod`,
# but not when accessed on the class object `NominalWithClassMethod` itself
#
# TODO: these should pass
static_assert(not is_assignable_to(NominalWithClassMethod, P))  # error: [static-assert-error]
static_assert(not is_assignable_to(NominalSubtype | NominalWithClassMethod, P))  # error: [static-assert-error]

# Conversely, `m` has the correct signature when accessed on the class object
# `NominalWithStaticMethod`, but not when accessed on instances of `NominalWithStaticMethod`
static_assert(not is_assignable_to(NominalWithStaticMethod, P))
static_assert(not is_assignable_to(NominalSubtype | NominalWithStaticMethod, P))
```

A callable instance attribute is not sufficient for a type to satisfy a protocol with a method
member: a method member specified by a protocol `P` must exist on the *meta-type* of `T` for `T` to
be a subtype of `P`:

```py
from typing import Callable, Protocol
from ty_extensions import static_assert, is_assignable_to

class SupportsFooMethod(Protocol):
    def foo(self): ...

class SupportsFooAttr(Protocol):
    foo: Callable[..., object]

class Foo:
    def __init__(self):
        self.foo: Callable[..., object] = lambda *args, **kwargs: None

static_assert(not is_assignable_to(Foo, SupportsFooMethod))
static_assert(is_assignable_to(Foo, SupportsFooAttr))
```

The reason for this is that some methods, such as dunder methods, are always looked up on the class
directly. If a class with an `__iter__` instance attribute satisfied the `Iterable` protocol, for
example, the `Iterable` protocol would not accurately describe the requirements Python has for a
class to be iterable at runtime. Allowing callable instance attributes to satisfy method members of
protocols would also make `issubclass()` narrowing of runtime-checkable protocols unsound, as the
`issubclass()` mechanism at runtime for protocols only checks whether a method is accessible on the
class object, not the instance. (Protocols with non-method members cannot be passed to
`issubclass()` at all at runtime.)

```py
from typing import Iterable, Any
from ty_extensions import static_assert, is_assignable_to

class Foo:
    def __init__(self):
        self.__iter__: Callable[..., object] = lambda *args, **kwargs: None

static_assert(not is_assignable_to(Foo, Iterable[Any]))
```

Because method members are always looked up on the meta-type of an object when testing assignability
and subtyping, we understand that `IterableClass` here is a subtype of `Iterable[int]` even though
`IterableClass.__iter__` has the wrong signature:

```py
from typing import Iterator, Iterable
from ty_extensions import static_assert, is_subtype_of, TypeOf

class Meta(type):
    def __iter__(self) -> Iterator[int]:
        yield from range(42)

class IterableClass(metaclass=Meta):
    def __iter__(self) -> Iterator[str]:
        yield from "abc"

static_assert(is_subtype_of(TypeOf[IterableClass], Iterable[int]))
```

Enforcing that members must always be available on the class also means that it is safe to access a
method on `type[P]`, where `P` is a protocol class, just like it is generally safe to access a
method on `type[C]` where `C` is a nominal class:

```py
from typing import Protocol

class Foo(Protocol):
    def method(self) -> str: ...

def f(x: Foo):
    reveal_type(type(x).method)  # revealed: def method(self, /) -> str

class Bar:
    def __init__(self):
        self.method = lambda: "foo"

f(Bar())  # error: [invalid-argument-type]
```

Some protocols use the old convention (specified in PEP-484) for denoting positional-only
parameters. This is supported by ty:

```py
class HasPosOnlyDunders:
    def __invert__(self, /) -> "HasPosOnlyDunders":
        return self

    def __lt__(self, other, /) -> bool:
        return True

class SupportsLessThan(Protocol):
    def __lt__(self, __other) -> bool: ...

class Invertable(Protocol):
    # `self` and `cls` are always implicitly positional-only for methods defined in `Protocol`
    # classes, even if no parameters in the method use the PEP-484 convention.
    def __invert__(self) -> object: ...

static_assert(is_assignable_to(HasPosOnlyDunders, SupportsLessThan))
static_assert(is_assignable_to(HasPosOnlyDunders, Invertable))
static_assert(is_assignable_to(str, SupportsLessThan))
static_assert(is_assignable_to(int, Invertable))
```

## Subtyping of protocols with generic method members

Protocol method members can be generic. They can have generic contexts scoped to the class:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import final
from typing_extensions import TypeVar, Self, Protocol
from ty_extensions import is_equivalent_to, static_assert, is_assignable_to, is_subtype_of

class NewStyleClassScoped[T](Protocol):
    def method(self, input: T) -> None: ...

S = TypeVar("S")

class LegacyClassScoped(Protocol[S]):
    def method(self, input: S) -> None: ...

# TODO: these should pass
static_assert(is_equivalent_to(NewStyleClassScoped, LegacyClassScoped))  # error: [static-assert-error]
static_assert(is_equivalent_to(NewStyleClassScoped[int], LegacyClassScoped[int]))  # error: [static-assert-error]

class NominalGeneric[T]:
    def method(self, input: T) -> None: ...

def _[T](x: T) -> T:
    # TODO: should pass
    static_assert(is_equivalent_to(NewStyleClassScoped[T], LegacyClassScoped[T]))  # error: [static-assert-error]
    static_assert(is_subtype_of(NominalGeneric[T], NewStyleClassScoped[T]))
    static_assert(is_subtype_of(NominalGeneric[T], LegacyClassScoped[T]))
    return x

class NominalConcrete:
    def method(self, input: int) -> None: ...

static_assert(is_assignable_to(NominalConcrete, NewStyleClassScoped))
static_assert(is_assignable_to(NominalConcrete, LegacyClassScoped))
static_assert(is_assignable_to(NominalGeneric[int], NewStyleClassScoped))
static_assert(is_assignable_to(NominalGeneric[int], LegacyClassScoped))
static_assert(is_assignable_to(NominalGeneric, NewStyleClassScoped[int]))
static_assert(is_assignable_to(NominalGeneric, LegacyClassScoped[int]))

# `NewStyleClassScoped` is implicitly `NewStyleClassScoped[Unknown]`,
# and there exist fully static materializations of `NewStyleClassScoped[Unknown]`
# where `Nominal` would not be a subtype of the given materialization,
# hence there is no subtyping relation:
static_assert(not is_subtype_of(NominalConcrete, NewStyleClassScoped))
static_assert(not is_subtype_of(NominalConcrete, LegacyClassScoped))

# Similarly, `NominalGeneric` is implicitly `NominalGeneric[Unknown`]
static_assert(not is_subtype_of(NominalGeneric, NewStyleClassScoped[int]))
static_assert(not is_subtype_of(NominalGeneric, LegacyClassScoped[int]))

static_assert(is_subtype_of(NominalConcrete, NewStyleClassScoped[int]))
static_assert(is_subtype_of(NominalConcrete, LegacyClassScoped[int]))
static_assert(is_subtype_of(NominalGeneric[int], NewStyleClassScoped[int]))
static_assert(is_subtype_of(NominalGeneric[int], LegacyClassScoped[int]))

static_assert(not is_assignable_to(NominalConcrete, NewStyleClassScoped[str]))
static_assert(not is_assignable_to(NominalConcrete, LegacyClassScoped[str]))
static_assert(not is_subtype_of(NominalGeneric[int], NewStyleClassScoped[str]))
static_assert(not is_subtype_of(NominalGeneric[int], LegacyClassScoped[str]))
```

And they can also have generic contexts scoped to the method:

```py
class NewStyleFunctionScoped(Protocol):
    def f[T](self, input: T) -> T: ...

S = TypeVar("S")

class LegacyFunctionScoped(Protocol):
    def f(self, input: S) -> S: ...

class UsesSelf(Protocol):
    def g(self: Self) -> Self: ...

class NominalNewStyle:
    def f[T](self, input: T) -> T:
        return input

class NominalLegacy:
    def f(self, input: S) -> S:
        return input

class NominalWithSelf:
    def g(self: Self) -> Self:
        return self

class NominalNotGeneric:
    def f(self, input: int) -> int:
        return input

class NominalReturningSelfNotGeneric:
    def g(self) -> "NominalReturningSelfNotGeneric":
        return self

@final
class Other: ...

class NominalReturningOtherClass:
    def g(self) -> Other:
        raise NotImplementedError

# TODO: should pass
static_assert(is_equivalent_to(LegacyFunctionScoped, NewStyleFunctionScoped))  # error: [static-assert-error]

static_assert(is_assignable_to(NominalNewStyle, NewStyleFunctionScoped))
static_assert(is_assignable_to(NominalNewStyle, LegacyFunctionScoped))
static_assert(is_subtype_of(NominalNewStyle, NewStyleFunctionScoped))
static_assert(is_subtype_of(NominalNewStyle, LegacyFunctionScoped))
static_assert(not is_assignable_to(NominalNewStyle, UsesSelf))

static_assert(is_assignable_to(NominalLegacy, NewStyleFunctionScoped))
static_assert(is_assignable_to(NominalLegacy, LegacyFunctionScoped))
static_assert(is_subtype_of(NominalLegacy, NewStyleFunctionScoped))
static_assert(is_subtype_of(NominalLegacy, LegacyFunctionScoped))
static_assert(not is_assignable_to(NominalLegacy, UsesSelf))

static_assert(not is_assignable_to(NominalWithSelf, NewStyleFunctionScoped))
static_assert(not is_assignable_to(NominalWithSelf, LegacyFunctionScoped))
static_assert(is_assignable_to(NominalWithSelf, UsesSelf))
static_assert(is_subtype_of(NominalWithSelf, UsesSelf))

# TODO: these should pass
static_assert(not is_assignable_to(NominalNotGeneric, NewStyleFunctionScoped))  # error: [static-assert-error]
static_assert(not is_assignable_to(NominalNotGeneric, LegacyFunctionScoped))  # error: [static-assert-error]
static_assert(not is_assignable_to(NominalNotGeneric, UsesSelf))

static_assert(not is_assignable_to(NominalReturningSelfNotGeneric, NewStyleFunctionScoped))
static_assert(not is_assignable_to(NominalReturningSelfNotGeneric, LegacyFunctionScoped))

# TODO: should pass
static_assert(not is_assignable_to(NominalReturningSelfNotGeneric, UsesSelf))  # error: [static-assert-error]

static_assert(not is_assignable_to(NominalReturningOtherClass, UsesSelf))

# These test cases are taken from the typing conformance suite:
class ShapeProtocolImplicitSelf(Protocol):
    def set_scale(self, scale: float) -> Self: ...

class ShapeProtocolExplicitSelf(Protocol):
    def set_scale(self: Self, scale: float) -> Self: ...

class BadReturnType:
    def set_scale(self, scale: float) -> int:
        return 42

static_assert(not is_assignable_to(BadReturnType, ShapeProtocolImplicitSelf))
static_assert(not is_assignable_to(BadReturnType, ShapeProtocolExplicitSelf))
```

## Subtyping of protocols with `@classmethod` or `@staticmethod` members

The typing spec states that protocols may have `@classmethod` or `@staticmethod` method members.
However, as of 2025/09/24, the spec does not elaborate on how these members should behave with
regards to subtyping and assignability (nor are there any tests in the typing conformance suite).
Ty's behaviour is therefore derived from first principles and the
[mypy test suite](https://github.com/python/mypy/blob/354bea6352ee7a38b05e2f42c874e7d1f7bf557a/test-data/unit/check-protocols.test#L1231-L1263).

A protocol `P` with a `@classmethod` method member `x` can only be satisfied by a nominal type `N`
if `N.x` is a callable object that evaluates to the same type whether it is accessed on inhabitants
of `N` or inhabitants of `type[N]`, *and* the signature of `N.x` is equivalent to the signature of
`P.x` after the descriptor protocol has been invoked on `P.x`:

```py
from typing import Protocol
from ty_extensions import static_assert, is_subtype_of, is_assignable_to, is_equivalent_to, is_disjoint_from

class PClassMethod(Protocol):
    @classmethod
    def x(cls, val: int) -> str: ...

class PStaticMethod(Protocol):
    @staticmethod
    def x(val: int) -> str: ...

class NNotCallable:
    x = None

class NInstanceMethod:
    def x(self, val: int) -> str:
        return "foo"

class NClassMethodGood:
    @classmethod
    def x(cls, val: int) -> str:
        return "foo"

class NClassMethodBad:
    @classmethod
    def x(cls, val: str) -> int:
        return 42

class NStaticMethodGood:
    @staticmethod
    def x(val: int) -> str:
        return "foo"

class NStaticMethodBad:
    @staticmethod
    def x(cls, val: int) -> str:
        return "foo"

# `PClassMethod.x` and `PStaticMethod.x` evaluate to callable types with equivalent signatures
# whether you access them on the protocol class or instances of the protocol.
# That means that they are equivalent protocols!
static_assert(is_equivalent_to(PClassMethod, PStaticMethod))

# TODO: these should all pass
static_assert(not is_assignable_to(NNotCallable, PClassMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NNotCallable, PStaticMethod))  # error: [static-assert-error]
static_assert(is_disjoint_from(NNotCallable, PClassMethod))  # error: [static-assert-error]
static_assert(is_disjoint_from(NNotCallable, PStaticMethod))  # error: [static-assert-error]

# `NInstanceMethod.x` has the correct type when accessed on an instance of
# `NInstanceMethod`, but not when accessed on the class object itself
#
# TODO: these should pass
static_assert(not is_assignable_to(NInstanceMethod, PClassMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NInstanceMethod, PStaticMethod))  # error: [static-assert-error]

# A nominal type with a `@staticmethod` can satisfy a protocol with a `@classmethod`
# if the staticmethod duck-types the same as the classmethod member
# both when accessed on the class and when accessed on an instance of the class
# The same also applies for a nominal type with a `@classmethod` and a protocol
# with a `@staticmethod` member
static_assert(is_assignable_to(NClassMethodGood, PClassMethod))
static_assert(is_assignable_to(NClassMethodGood, PStaticMethod))
# TODO: these should all pass:
static_assert(is_subtype_of(NClassMethodGood, PClassMethod))  # error: [static-assert-error]
static_assert(is_subtype_of(NClassMethodGood, PStaticMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NClassMethodBad, PClassMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NClassMethodBad, PStaticMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NClassMethodGood | NClassMethodBad, PClassMethod))  # error: [static-assert-error]

static_assert(is_assignable_to(NStaticMethodGood, PClassMethod))
static_assert(is_assignable_to(NStaticMethodGood, PStaticMethod))
# TODO: these should all pass:
static_assert(is_subtype_of(NStaticMethodGood, PClassMethod))  # error: [static-assert-error]
static_assert(is_subtype_of(NStaticMethodGood, PStaticMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NStaticMethodBad, PClassMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NStaticMethodBad, PStaticMethod))  # error: [static-assert-error]
static_assert(not is_assignable_to(NStaticMethodGood | NStaticMethodBad, PStaticMethod))  # error: [static-assert-error]
```

## Equivalence of protocols with method or property members

Two protocols `P1` and `P2`, both with a method member `x`, are considered equivalent if the
signature of `P1.x` is equivalent to the signature of `P2.x`, even though ty would normally model
any two function definitions as inhabiting distinct function-literal types. The same is also true
for property members.

```py
from typing import Protocol
from ty_extensions import is_equivalent_to, static_assert

class P1(Protocol):
    def x(self, y: int) -> None: ...

class P2(Protocol):
    def x(self, y: int) -> None: ...

class P3(Protocol):
    @property
    def y(self) -> str: ...
    @property
    def z(self) -> bytes: ...
    @z.setter
    def z(self, value: int) -> None: ...

class P4(Protocol):
    @property
    def y(self) -> str: ...
    @property
    def z(self) -> bytes: ...
    @z.setter
    def z(self, value: int) -> None: ...

static_assert(is_equivalent_to(P1, P2))

# TODO: should pass
static_assert(is_equivalent_to(P3, P4))  # error: [static-assert-error]
```

As with protocols that only have non-method members, this also holds true when they appear in
differently ordered unions:

```py
class A: ...
class B: ...

static_assert(is_equivalent_to(A | B | P1, P2 | B | A))

# TODO: should pass
static_assert(is_equivalent_to(A | B | P3, P4 | B | A))  # error: [static-assert-error]
```

## Subtyping between two protocol types with method members

A protocol `PSub` with a method member can be considered a subtype of a protocol `PSuper` with a
method member if the signature of the member on `PSub` is a subtype of the signature of the member
on `PSuper`:

```py
from typing import Protocol
from ty_extensions import static_assert, is_subtype_of, is_assignable_to

class Super: ...
class Sub(Super): ...
class Unrelated: ...

class MethodPSuper(Protocol):
    def f(self) -> Super: ...

class MethodPSub(Protocol):
    def f(self) -> Sub: ...

class MethodPUnrelated(Protocol):
    def f(self) -> Unrelated: ...

static_assert(is_subtype_of(MethodPSub, MethodPSuper))

static_assert(not is_assignable_to(MethodPUnrelated, MethodPSuper))
static_assert(not is_assignable_to(MethodPSuper, MethodPUnrelated))
static_assert(not is_assignable_to(MethodPSuper, MethodPSub))
```

## Subtyping between protocols with method members and protocols with non-method members

A protocol with a method member can be considered a subtype of a protocol with a read-only
`@property` member that returns a `Callable` type:

```py
from typing import Protocol, Callable
from ty_extensions import static_assert, is_subtype_of, is_assignable_to

class PropertyInt(Protocol):
    @property
    def f(self) -> Callable[[], int]: ...

class PropertyBool(Protocol):
    @property
    def f(self) -> Callable[[], bool]: ...

class PropertyNotReturningCallable(Protocol):
    @property
    def f(self) -> int: ...

class PropertyWithIncorrectSignature(Protocol):
    @property
    def f(self) -> Callable[[object], int]: ...

class Method(Protocol):
    def f(self) -> bool: ...

static_assert(is_subtype_of(Method, PropertyInt))
static_assert(is_subtype_of(Method, PropertyBool))

# TODO: these should pass
static_assert(not is_assignable_to(Method, PropertyNotReturningCallable))  # error: [static-assert-error]
static_assert(not is_assignable_to(Method, PropertyWithIncorrectSignature))  # error: [static-assert-error]
```

However, a protocol with a method member can never be considered a subtype of a protocol with a
writable property member of the same name, as method members are covariant and immutable:

```py
class ReadWriteProperty(Protocol):
    @property
    def f(self) -> Callable[[], bool]: ...
    @f.setter
    def f(self, val: Callable[[], bool]): ...

# TODO: should pass
static_assert(not is_assignable_to(Method, ReadWriteProperty))  # error: [static-assert-error]
```

And for the same reason, they are never assignable to attribute members (which are also mutable):

```py
class Attribute(Protocol):
    f: Callable[[], bool]

static_assert(not is_assignable_to(Method, Attribute))
```

Protocols with attribute members, meanwhile, cannot be assigned to protocols with method members,
since a method member is guaranteed to exist on the meta-type as well as the instance type, whereas
this is not true for attribute members. The same principle also applies for protocols with property
members

```py
static_assert(not is_assignable_to(PropertyBool, Method))
static_assert(not is_assignable_to(Attribute, Method))
```

But an exception to this rule is if an attribute member is marked as `ClassVar`, as this guarantees
that the member will be available on the meta-type as well as the instance type for inhabitants of
the protocol:

```py
from typing import ClassVar

class ClassVarAttribute(Protocol):
    f: ClassVar[Callable[[], bool]]

static_assert(is_subtype_of(ClassVarAttribute, Method))
static_assert(is_assignable_to(ClassVarAttribute, Method))

class ClassVarAttributeBad(Protocol):
    f: ClassVar[Callable[[], str]]

static_assert(not is_subtype_of(ClassVarAttributeBad, Method))
static_assert(not is_assignable_to(ClassVarAttributeBad, Method))
```

## Narrowing of protocols

<!-- snapshot-diagnostics -->

By default, a protocol class cannot be used as the second argument to `isinstance()` or
`issubclass()`, and a type checker must emit an error on such calls. However, we still narrow the
type inside these branches (this matches the behavior of other type checkers):

```py
from typing_extensions import Protocol

class HasX(Protocol):
    x: int

def f(arg: object, arg2: type):
    if isinstance(arg, HasX):  # error: [invalid-argument-type]
        reveal_type(arg)  # revealed: HasX
    else:
        reveal_type(arg)  # revealed: ~HasX

    if issubclass(arg2, HasX):  # error: [invalid-argument-type]
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
    if issubclass(arg1, RuntimeCheckableHasX):  # TODO: should emit an error here (has non-method members)
        reveal_type(arg1)  # revealed: type[RuntimeCheckableHasX]
    else:
        reveal_type(arg1)  # revealed: type & ~type[RuntimeCheckableHasX]

    if issubclass(arg2, OnlyMethodMembers):  # no error!
        reveal_type(arg2)  # revealed: type[OnlyMethodMembers]
    else:
        reveal_type(arg2)  # revealed: type & ~type[OnlyMethodMembers]
```

## Truthiness of protocol instances

An instance of a protocol type generally has ambiguous truthiness:

```py
from typing import Protocol

class Foo(Protocol):
    x: int

def f(foo: Foo):
    reveal_type(bool(foo))  # revealed: bool
```

But this is not the case if the protocol has a `__bool__` method member that returns `Literal[True]`
or `Literal[False]`:

```py
from typing import Literal

class Truthy(Protocol):
    def __bool__(self) -> Literal[True]: ...

class FalsyFoo(Foo, Protocol):
    def __bool__(self) -> Literal[False]: ...

class FalsyFooSubclass(FalsyFoo, Protocol):
    y: str

def g(a: Truthy, b: FalsyFoo, c: FalsyFooSubclass):
    reveal_type(bool(a))  # revealed: Literal[True]
    reveal_type(bool(b))  # revealed: Literal[False]
    reveal_type(bool(c))  # revealed: Literal[False]
```

The same works with a class-level declaration of `__bool__`:

```py
from typing import Callable

class InstanceAttrBool(Protocol):
    __bool__: Callable[[], Literal[True]]

def h(obj: InstanceAttrBool):
    reveal_type(bool(obj))  # revealed: Literal[True]
```

## Callable protocols

An instance of a protocol type is callable if the protocol defines a `__call__` method:

```py
from typing import Protocol

class CallMeMaybe(Protocol):
    def __call__(self, x: int) -> str: ...

def f(obj: CallMeMaybe):
    reveal_type(obj(42))  # revealed: str
    obj("bar")  # error: [invalid-argument-type]
```

An instance of a protocol like this can be assignable to a `Callable` type, but only if it has the
right signature:

```py
from typing import Callable
from ty_extensions import is_subtype_of, is_assignable_to, static_assert

static_assert(is_subtype_of(CallMeMaybe, Callable[[int], str]))
static_assert(is_assignable_to(CallMeMaybe, Callable[[int], str]))
static_assert(not is_subtype_of(CallMeMaybe, Callable[[str], str]))
static_assert(not is_assignable_to(CallMeMaybe, Callable[[str], str]))
static_assert(not is_subtype_of(CallMeMaybe, Callable[[CallMeMaybe, int], str]))
static_assert(not is_assignable_to(CallMeMaybe, Callable[[CallMeMaybe, int], str]))

def g(obj: Callable[[int], str], obj2: CallMeMaybe, obj3: Callable[[str], str]):
    obj = obj2
    obj3 = obj2  # error: [invalid-assignment]
```

By the same token, a `Callable` type can also be assignable to a protocol-instance type if the
signature implied by the `Callable` type is assignable to the signature of the `__call__` method
specified by the protocol:

```py
from ty_extensions import TypeOf

class Foo(Protocol):
    def __call__(self, x: int, /) -> str: ...

static_assert(is_subtype_of(Callable[[int], str], Foo))
static_assert(is_assignable_to(Callable[[int], str], Foo))

static_assert(not is_subtype_of(Callable[[str], str], Foo))
static_assert(not is_assignable_to(Callable[[str], str], Foo))
static_assert(not is_subtype_of(Callable[[CallMeMaybe, int], str], Foo))
static_assert(not is_assignable_to(Callable[[CallMeMaybe, int], str], Foo))

def h(obj: Callable[[int], str], obj2: Foo, obj3: Callable[[str], str]):
    obj2 = obj

    # error: [invalid-assignment] "Object of type `(str, /) -> str` is not assignable to `Foo`"
    obj2 = obj3

def satisfies_foo(x: int) -> str:
    return "foo"

static_assert(is_assignable_to(TypeOf[satisfies_foo], Foo))
static_assert(is_subtype_of(TypeOf[satisfies_foo], Foo))

def doesnt_satisfy_foo(x: str) -> int:
    return 42

static_assert(not is_assignable_to(TypeOf[doesnt_satisfy_foo], Foo))
static_assert(not is_subtype_of(TypeOf[doesnt_satisfy_foo], Foo))
```

Class-literals and generic aliases can also be subtypes of callback protocols:

```py
from typing import Sequence, TypeVar

static_assert(is_subtype_of(TypeOf[str], Foo))

T = TypeVar("T")

class SequenceMaker(Protocol[T]):
    def __call__(self, arg: Sequence[T], /) -> Sequence[T]: ...

static_assert(is_subtype_of(TypeOf[list[int]], SequenceMaker[int]))

# TODO: these should pass
static_assert(is_subtype_of(TypeOf[tuple[str, ...]], SequenceMaker[str]))  # error: [static-assert-error]
static_assert(is_subtype_of(TypeOf[tuple[str, ...]], SequenceMaker[int | str]))  # error: [static-assert-error]
```

## Nominal subtyping of protocols

Protocols can participate in nominal subtyping as well as structural subtyping. The main use case
for this is that it allows users an "escape hatch" to force a type checker to consider another type
to be a subtype of a given protocol, even if the other type violates the Liskov Substitution
Principle in some way.

```py
from typing import Protocol, final
from ty_extensions import static_assert, is_subtype_of, is_disjoint_from

class X(Protocol):
    x: int

class YProto(X, Protocol):
    x: None = None  # TODO: we should emit an error here due to the Liskov violation

@final
class YNominal(X):
    x: None = None  # TODO: we should emit an error here due to the Liskov violation

static_assert(is_subtype_of(YProto, X))
static_assert(is_subtype_of(YNominal, X))
static_assert(not is_disjoint_from(YProto, X))
static_assert(not is_disjoint_from(YNominal, X))
```

A common use case for this behaviour is that a lot of ecosystem code depends on type checkers
considering `str` to be a subtype of `Container[str]`. From a structural-subtyping perspective, this
is not the case, since `str.__contains__` only accepts `str`, while the `Container` interface
specifies that a type must have a `__contains__` method which accepts `object` in order for that
type to be considered a subtype of `Container`. Nonetheless, `str` has `Container[str]` in its MRO,
and other type checkers therefore consider it to be a subtype of `Container[str]` -- as such, so do
we:

```py
from typing import Container

static_assert(is_subtype_of(str, Container[str]))
static_assert(not is_disjoint_from(str, Container[str]))
```

This behaviour can have some counter-intuitive repercussions. For example, one implication of this
is that not all subtype of `Iterable` are necessarily considered iterable by ty if a given subtype
violates the Liskov principle (this also matches the behaviour of other type checkers):

```py
from typing import Iterable

class Foo(Iterable[int]):
    __iter__ = None

static_assert(is_subtype_of(Foo, Iterable[int]))

def _(x: Foo):
    for item in x:  # error: [not-iterable]
        pass
```

## Protocols are never singleton types, and are never single-valued types

It *might* be possible to have a singleton protocol-instance type...?

For example, `WeirdAndWacky` in the following snippet only has a single possible inhabitant: `None`!
It is thus a singleton type. However, going out of our way to recognize it as such is probably not
worth it. Such cases should anyway be exceedingly rare and/or contrived.

```py
from typing import Protocol, Callable
from ty_extensions import is_singleton, is_single_valued

class WeirdAndWacky(Protocol):
    @property
    def __class__(self) -> Callable[[], None]: ...

reveal_type(is_singleton(WeirdAndWacky))  # revealed: Literal[False]
reveal_type(is_single_valued(WeirdAndWacky))  # revealed: Literal[False]
```

## Integration test: `typing.SupportsIndex` and `typing.Sized`

`typing.SupportsIndex` and `typing.Sized` are two protocols that are very commonly used in the wild.

```py
from typing import SupportsIndex, Sized, Literal

def one(some_int: int, some_literal_int: Literal[1], some_indexable: SupportsIndex):
    a: SupportsIndex = some_int
    b: SupportsIndex = some_literal_int
    c: SupportsIndex = some_indexable

def two(some_list: list, some_tuple: tuple[int, str], some_sized: Sized):
    a: Sized = some_list
    b: Sized = some_tuple
    c: Sized = some_sized
```

## Recursive protocols

### Properties

```py
from __future__ import annotations

from typing import Protocol, Any, TypeVar
from ty_extensions import static_assert, is_assignable_to, is_subtype_of, is_equivalent_to

class RecursiveFullyStatic(Protocol):
    parent: RecursiveFullyStatic
    x: int

class RecursiveNonFullyStatic(Protocol):
    parent: RecursiveNonFullyStatic
    x: Any

static_assert(not is_subtype_of(RecursiveFullyStatic, RecursiveNonFullyStatic))
static_assert(not is_subtype_of(RecursiveNonFullyStatic, RecursiveFullyStatic))

static_assert(is_assignable_to(RecursiveNonFullyStatic, RecursiveNonFullyStatic))
static_assert(is_assignable_to(RecursiveFullyStatic, RecursiveNonFullyStatic))
static_assert(is_assignable_to(RecursiveNonFullyStatic, RecursiveFullyStatic))

class AlsoRecursiveFullyStatic(Protocol):
    parent: AlsoRecursiveFullyStatic
    x: int

static_assert(is_equivalent_to(AlsoRecursiveFullyStatic, RecursiveFullyStatic))

class RecursiveOptionalParent(Protocol):
    parent: RecursiveOptionalParent | None

static_assert(is_assignable_to(RecursiveOptionalParent, RecursiveOptionalParent))

# Due to invariance of mutable attribute members, neither is assignable to the other
static_assert(not is_assignable_to(RecursiveNonFullyStatic, RecursiveOptionalParent))
static_assert(not is_assignable_to(RecursiveOptionalParent, RecursiveNonFullyStatic))

class Other(Protocol):
    z: str

def _(rec: RecursiveFullyStatic, other: Other):
    reveal_type(rec.parent.parent.parent)  # revealed: RecursiveFullyStatic

    rec.parent.parent.parent = rec
    rec = rec.parent.parent.parent

    rec.parent.parent.parent = other  # error: [invalid-assignment]
    other = rec.parent.parent.parent  # error: [invalid-assignment]

class Foo(Protocol):
    @property
    def x(self) -> "Foo": ...

class Bar(Protocol):
    @property
    def x(self) -> "Bar": ...

# TODO: this should pass
# error: [static-assert-error]
static_assert(is_equivalent_to(Foo, Bar))

T = TypeVar("T", bound="TypeVarRecursive")

class TypeVarRecursive(Protocol):
    # TODO: commenting this out will cause a stack overflow.
    # x: T
    y: "TypeVarRecursive"

def _(t: TypeVarRecursive):
    # reveal_type(t.x)  # revealed: T
    reveal_type(t.y)  # revealed: TypeVarRecursive
```

### Nested occurrences of self-reference

Make sure that we handle self-reference correctly, even if the self-reference appears deeply nested
within the type of a protocol member:

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

from typing import Protocol, Callable
from ty_extensions import Intersection, Not, is_assignable_to, is_equivalent_to, static_assert

class C: ...

class GenericC[T](Protocol):
    pass

class Recursive(Protocol):
    direct: Recursive

    union: None | Recursive

    intersection1: Intersection[C, Recursive]
    intersection2: Intersection[C, Not[Recursive]]

    t: tuple[int, tuple[str, Recursive]]

    callable1: Callable[[int], Recursive]
    callable2: Callable[[Recursive], int]

    subtype_of: type[Recursive]

    generic: GenericC[Recursive]

    def method(self, x: Recursive) -> Recursive: ...

    nested: Recursive | Callable[[Recursive | Recursive, tuple[Recursive, Recursive]], Recursive | Recursive]

static_assert(is_equivalent_to(Recursive, Recursive))
static_assert(is_assignable_to(Recursive, Recursive))

def _(r: Recursive):
    reveal_type(r.direct)  # revealed: Recursive
    reveal_type(r.union)  # revealed: None | Recursive
    reveal_type(r.intersection1)  # revealed: C & Recursive
    reveal_type(r.intersection2)  # revealed: C
    reveal_type(r.t)  # revealed: tuple[int, tuple[str, Recursive]]
    reveal_type(r.callable1)  # revealed: (int, /) -> Recursive
    reveal_type(r.callable2)  # revealed: (Recursive, /) -> int
    reveal_type(r.subtype_of)  # revealed: @Todo(type[T] for protocols)
    reveal_type(r.generic)  # revealed: GenericC[Recursive]
    reveal_type(r.method(r))  # revealed: Recursive
    reveal_type(r.nested)  # revealed: Recursive | ((Recursive, tuple[Recursive, Recursive], /) -> Recursive)

    reveal_type(r.method(r).callable1(1).direct.t[1][1])  # revealed: Recursive
```

### Mutually-recursive protocols

```py
from typing import Protocol
from ty_extensions import is_equivalent_to, static_assert

class Foo(Protocol):
    x: "Bar"

class Bar(Protocol):
    x: Foo

static_assert(is_equivalent_to(Foo, Bar))
```

### Disjointness of recursive protocol and recursive final type

```py
from typing import Protocol
from ty_extensions import is_disjoint_from, static_assert

class Proto(Protocol):
    x: "Proto"

class Nominal:
    x: "Nominal"

static_assert(not is_disjoint_from(Proto, Nominal))
```

### Regression test: narrowing with self-referential protocols

This snippet caused us to panic on an early version of the implementation for protocols.

```py
from typing import Protocol

class A(Protocol):
    def x(self) -> "B | A": ...

class B(Protocol):
    def y(self): ...

obj = something_unresolvable  # error: [unresolved-reference]
reveal_type(obj)  # revealed: Unknown
if isinstance(obj, (B, A)):
    reveal_type(obj)  # revealed: (Unknown & B) | (Unknown & A)
```

### Protocols that use `Self`

`Self` is a `TypeVar` with an upper bound of the class in which it is defined. This means that
`Self` annotations in protocols can also be tricky to handle without infinite recursion and stack
overflows.

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import Protocol, Self
from ty_extensions import static_assert

class _HashObject(Protocol):
    def copy(self) -> Self: ...

class Foo: ...

# Attempting to build this union caused us to overflow on an early version of
# <https://github.com/astral-sh/ruff/pull/18659>
x: Foo | _HashObject
```

Some other similar cases that caused issues in our early `Protocol` implementation:

`a.py`:

```py
from typing_extensions import Protocol, Self

class PGconn(Protocol):
    def connect(self) -> Self: ...

class Connection:
    pgconn: PGconn

def is_crdb(conn: PGconn) -> bool:
    return isinstance(conn, Connection)
```

and:

`b.py`:

```py
from typing_extensions import Protocol

class PGconn(Protocol):
    def connect[T: PGconn](self: T) -> T: ...

class Connection:
    pgconn: PGconn

def f(x: PGconn):
    isinstance(x, Connection)
```

### Recursive protocols used as the first argument to `cast()`

These caused issues in an early version of our `Protocol` implementation due to the fact that we use
a recursive function in our `cast()` implementation to check whether a type contains `Unknown` or
`Todo`. Recklessly recursing into a type causes stack overflows if the type is recursive:

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations
from typing import cast, Protocol

class Iterator[T](Protocol):
    def __iter__(self) -> Iterator[T]: ...

def f(value: Iterator):
    cast(Iterator, value)  # error: [redundant-cast]
```

### Recursive generic protocols

This snippet caused us to stack overflow on an early version of
<https://github.com/astral-sh/ruff/pull/19866>:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, TypeVar

class A: ...

class Foo[T](Protocol):
    def x(self) -> "T | Foo[T]": ...

y: A | Foo[A]

# The same thing, but using the legacy syntax:

S = TypeVar("S")

class Bar(Protocol[S]):
    def x(self) -> "S | Bar[S]": ...

z: S | Bar[S]
```

### Recursive generic protocols with growing specializations

This snippet caused a stack overflow in <https://github.com/astral-sh/ty/issues/1736> because the
type parameter grows with each recursive call (`C[set[T]]` leads to `C[set[set[T]]]`, then
`C[set[set[set[T]]]]`, etc.):

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol

class C[T](Protocol):
    a: "C[set[T]]"

def takes_c(c: C[set[int]]) -> None: ...
def f(c: C[int]) -> None:
    # The key thing is that we don't stack overflow while checking this.
    # The cycle detection assumes compatibility when it detects potential
    # infinite recursion between protocol specializations.
    takes_c(c)
```

### Recursive legacy generic protocol

```py
from typing import Generic, TypeVar, Protocol

T = TypeVar("T")

class P(Protocol[T]):
    attr: "P[T] | T"

class A(Generic[T]):
    attr: T

class B(A[P[int]]):
    pass

def f(b: B):
    reveal_type(b)  # revealed: B
    reveal_type(b.attr)  # revealed: P[int]
    reveal_type(b.attr.attr)  # revealed: P[int] | int
```

### Recursive generic protocols with property members

An early version of <https://github.com/astral-sh/ruff/pull/19936> caused stack overflows on this
snippet:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol

class Foo[T]: ...

class A(Protocol):
    @property
    def _(self: "A") -> Foo: ...

class B(Protocol):
    @property
    def b(self) -> Foo[A]: ...

class C(Undefined): ...  # error: "Name `Undefined` used when not defined"

class D:
    b: Foo[C]

class E[T: B](Protocol): ...

x: E[D]
```

### Recursive supertypes of `object`

A recursive protocol can be a supertype of `object` (though it is hard to create such a protocol
without violating the Liskov Substitution Principle, since all protocols are also subtypes of
`object`):

```py
from typing import Protocol
from ty_extensions import static_assert, is_subtype_of, is_equivalent_to, is_disjoint_from

class HasRepr(Protocol):
    # error: [invalid-method-override]
    def __repr__(self) -> object: ...

class HasReprRecursive(Protocol):
    # error: [invalid-method-override]
    def __repr__(self) -> "HasReprRecursive": ...

class HasReprRecursiveAndFoo(Protocol):
    # error: [invalid-method-override]
    def __repr__(self) -> "HasReprRecursiveAndFoo": ...
    foo: int

static_assert(is_subtype_of(object, HasRepr))
static_assert(is_subtype_of(HasRepr, object))
static_assert(is_equivalent_to(object, HasRepr))
static_assert(not is_disjoint_from(HasRepr, object))

static_assert(is_subtype_of(object, HasReprRecursive))
static_assert(is_subtype_of(HasReprRecursive, object))
static_assert(is_equivalent_to(object, HasReprRecursive))
static_assert(not is_disjoint_from(HasReprRecursive, object))

static_assert(not is_subtype_of(object, HasReprRecursiveAndFoo))
static_assert(is_subtype_of(HasReprRecursiveAndFoo, object))
static_assert(not is_equivalent_to(object, HasReprRecursiveAndFoo))
static_assert(not is_disjoint_from(HasReprRecursiveAndFoo, object))
```

## Meta-protocols

Where `P` is a protocol type, a class object `N` can be said to inhabit the type `type[P]` if:

- All `ClassVar` members on `P` exist on the class object `N`
- All method members on `P` exist on the class object `N`
- Instantiating `N` creates an object that would satisfy the protocol `P`

Currently meta-protocols are not fully supported by ty, but we try to keep false positives to a
minimum in the meantime.

```py
from typing import Protocol, ClassVar
from ty_extensions import static_assert, is_assignable_to, TypeOf, is_subtype_of

class Foo(Protocol):
    x: int
    y: ClassVar[str]
    def method(self) -> bytes: ...

def _(f: type[Foo]):
    reveal_type(f)  # revealed: type[@Todo(type[T] for protocols)]

    # TODO: we should emit `unresolved-attribute` here: although we would accept this for a
    # nominal class, we would see any class `N` as inhabiting `Foo` if it had an implicit
    # instance attribute `x`, and implicit instance attributes are rarely bound on the class
    # object.
    reveal_type(f.x)  # revealed: @Todo(type[T] for protocols)

    # TODO: should be `str`
    reveal_type(f.y)  # revealed: @Todo(type[T] for protocols)
    f.y = "foo"  # fine

    # TODO: should be `Callable[[Foo], bytes]`
    reveal_type(f.method)  # revealed: @Todo(type[T] for protocols)

class Bar: ...

# TODO: these should pass
static_assert(not is_assignable_to(type[Bar], type[Foo]))  # error: [static-assert-error]
static_assert(not is_assignable_to(TypeOf[Bar], type[Foo]))  # error: [static-assert-error]

class Baz:
    x: int
    y: ClassVar[str] = "foo"
    def method(self) -> bytes:
        return b"foo"

static_assert(is_assignable_to(type[Baz], type[Foo]))
static_assert(is_assignable_to(TypeOf[Baz], type[Foo]))

# TODO: these should pass
static_assert(is_subtype_of(type[Baz], type[Foo]))  # error: [static-assert-error]
static_assert(is_subtype_of(TypeOf[Baz], type[Foo]))  # error: [static-assert-error]
```

## Regression test for `ClassVar` members in stubs

In an early version of our protocol implementation, we didn't retain the `ClassVar` qualifier for
protocols defined in stub files.

`stub.pyi`:

```pyi
from typing import ClassVar, Protocol

class Foo(Protocol):
    x: ClassVar[int]
```

`main.py`:

```py
from stub import Foo
from ty_extensions import reveal_protocol_interface

# revealed: {"x": AttributeMember(`int`; ClassVar)}
reveal_protocol_interface(Foo)
```

## Protocols generic over TypeVars bound to forward references

Protocols can have TypeVars with forward reference bounds that form cycles.

```py
from typing import Any, Protocol, TypeVar

T1 = TypeVar("T1", bound="A2[Any]")
T2 = TypeVar("T2", bound="A1[Any]")
T3 = TypeVar("T3", bound="B2[Any]")
T4 = TypeVar("T4", bound="B1[Any]")

class A1(Protocol[T1]):
    def get_x(self): ...

class A2(Protocol[T2]):
    def get_y(self): ...

class B1(A1[T3], Protocol[T3]): ...
class B2(A2[T4], Protocol[T4]): ...

# TODO should just be `B2[Any]`
reveal_type(T3.__bound__)  # revealed: B2[Any] | @Todo(specialized non-generic class)

# TODO error: [invalid-type-arguments]
def f(x: B1[int]):
    pass

reveal_type(T4.__bound__)  # revealed: B1[Any]

# error: [invalid-type-arguments]
def g(x: B2[int]):
    pass
```

## TODO

Add tests for:

- More tests for protocols inside `type[]`. [Spec reference][protocols_inside_type_spec].
- Protocols with instance-method members, including:
    - Protocols with methods that have parameters or the return type unannotated
    - Protocols with methods that have parameters or the return type annotated with `Any`
- Assignability of non-instance types to protocols with instance-method members (e.g. a
    class-literal type can be a subtype of `Sized` if its metaclass has a `__len__` method)
- Protocols with methods that have annotated `self` parameters.
    [Spec reference][self_types_protocols_spec].
- Protocols with overloaded method members
- `super()` on nominal subtypes (explicit and implicit) of protocol classes
- [Recursive protocols][recursive_protocols_spec]
- Generic protocols
- Protocols with instance attributes annotated with `Callable` (can a nominal type with a method
    satisfy that protocol, and if so in what cases?)
- Protocols decorated with `@final`
- Equivalence and subtyping between `Callable` types and protocols that define `__call__`

[mypy_protocol_docs]: https://mypy.readthedocs.io/en/stable/protocols.html#protocols-and-structural-subtyping
[mypy_protocol_tests]: https://github.com/python/mypy/blob/master/test-data/unit/check-protocols.test
[protocol conformance tests]: https://github.com/python/typing/tree/main/conformance/tests
[protocols_inside_type_spec]: https://typing.python.org/en/latest/spec/protocol.html#type-and-class-objects-vs-protocols
[recursive_protocols_spec]: https://typing.python.org/en/latest/spec/protocol.html#recursive-protocols
[self_types_protocols_spec]: https://typing.python.org/en/latest/spec/protocol.html#self-types-in-protocols
[spec_protocol_members]: https://typing.python.org/en/latest/spec/protocol.html#protocol-members
[typing_spec_protocols]: https://typing.python.org/en/latest/spec/protocol.html
