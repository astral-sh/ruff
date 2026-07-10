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
from ty_extensions._internal import reveal_mro

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

# Note that this class definition *will* actually succeed at runtime,
# but is banned by the typing spec anyway
# error: [invalid-generic-class] "Cannot both inherit from subscripted `Protocol` and subscripted `Generic`"
class Bar1(Protocol[T], Generic[T]):
    x: T

class Bar1Point5(Protocol, Generic[T]):
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

ProtocolT = TypeVar("ProtocolT")

# Note: pyright and pyrefly do not consider this to be a valid `Protocol` class,
# but mypy does (and has an explicit test for this behavior). Mypy was the
# reference implementation for PEP-544, and its behavior also matches the CPython
# runtime, so we choose to follow its behavior here rather than that of the other
# type checkers.
class Fine(Protocol, object): ...

reveal_mro(Fine)  # revealed: (<class 'Fine'>, typing.Protocol, typing.Generic, <class 'object'>)

class StillFine(Protocol, Generic[ProtocolT], object): ...
class EvenThis[T](Protocol, object): ...
class OrThis(Protocol, Generic[ProtocolT]): ...
class AndThis(Protocol, Generic[ProtocolT], object): ...
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
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

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
    x: Protocol,  # error: [invalid-type-form] "`typing.Protocol` is not allowed in parameter annotations"
    y: type[Protocol],  # error: [invalid-type-form] "`typing.Protocol` is not allowed in parameter annotations"
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
from ty_extensions._internal import TypeOf, reveal_mro

reveal_type(type(Protocol))  # revealed: <class '_ProtocolMeta'>
# revealed: (<class '_ProtocolMeta'>, <class 'ABCMeta'>, <class 'type'>, <class 'object'>)
reveal_mro(type(Protocol))
static_assert(is_subtype_of(TypeOf[Protocol], type))
static_assert(is_subtype_of(TypeOf[Protocol], abc.ABCMeta))
static_assert(is_subtype_of(TypeOf[Protocol], typing._ProtocolMeta))

# Could also be `Literal[True]`, but `bool` is fine:
reveal_type(issubclass(MyProtocol, Protocol))  # revealed: bool
```

## Diagnostics and autofixes for `Protocol` classes defined in invalid ways

<!-- snapshot-diagnostics -->

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, Generic, TypeVar

T = TypeVar("T")

class Foo(Protocol[T], Generic[T]): ...  # error: [invalid-generic-class]

# fmt: off

# error: [invalid-generic-class]
class Bar(Protocol[
  T,
], Generic[T]): ...

class Spam(  # docs
  # error: [invalid-generic-class]
  Protocol[  # some comment
    # another comment
    T,  # just love my comments
    # very well documented code
],  # important comma!
  # and a newline...
  Generic[  # look at this
  # wow
    T,  # wow
    # wowwwwwww
  ] # oof
  # another newline?
): ...

# fmt: on

class Foo[T](Protocol[T]): ...  # error: [invalid-generic-class]
```

## `typing.Protocol` versus `typing_extensions.Protocol`

`typing.Protocol` and its backport in `typing_extensions` should be treated as exactly equivalent.

```py
import typing
import typing_extensions
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_equivalent_to

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

reveal_type(get_protocol_members(Foo))  # revealed: frozenset[Literal["method_member", "x", "y", "z"]]
```

To see the kinds and types of the protocol members, you can use the debugging aid
`ty_extensions._internal.reveal_protocol_interface`, meanwhile:

```py
from ty_extensions._internal import reveal_protocol_interface
from typing import SupportsIndex, SupportsAbs, ClassVar, Iterator

# revealed: {"method_member": MethodMember(`(self, /) -> bytes`), "x": AttributeMember(`int`), "y": PropertyMember { read: `str` }, "z": PropertyMember { read: `int`, write: `int` }}
reveal_protocol_interface(Foo)
# revealed: {"method_member": MethodMember(`(self, /) -> bytes`), "x": AttributeMember(`int`), "y": PropertyMember { read: `str` }, "z": PropertyMember { read: `int`, write: `int` }}
reveal_protocol_interface(protocol=Foo)
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
`ty_extensions._internal.reveal_protocol_interface` can be used on both, however:

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
from typing import Protocol, Any, ClassVar, Final
from collections.abc import Sequence
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

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

# `FooSubclassOfAny` does not declare `__get__`, so `x` keeps its declared type instead of being
# read as `Any`.
static_assert(not is_assignable_to(FooSubclassOfAny, HasX))

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
    def __init__(self, x: int, y, flag: bool) -> None:
        self.x = x if flag else y

reveal_type(HalfUnknownQux(1, "foo", True).x)  # revealed: int | Unknown

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

static_assert(not is_subtype_of(Qux, HasClassVarX))
static_assert(not is_assignable_to(Qux, HasClassVarX))

class FinalClassVarX:
    x: Final[int] = 0

# A mutable ClassVar protocol member requires a writable class attribute.
static_assert(not is_subtype_of(FinalClassVarX, HasClassVarX))
static_assert(not is_assignable_to(FinalClassVarX, HasClassVarX))
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

static_assert(is_subtype_of(MutableDataclass, HasX))
static_assert(is_assignable_to(MutableDataclass, HasX))

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
    # `arg` may be an implicit subtype that does not define `x` on its class object.
    type(arg).x  # error: [unresolved-attribute]
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

Ordinary, annotated, and annotation-only assignments are treated the same:

```py
from typing import Any, ClassVar

class Foo(Protocol):
    x: int
    y: str

    def __init__(self) -> None:
        self.x = 42  # fine

        self.a = 56  # error: [ambiguous-protocol-member]
        self.b: int = 128  # error: [ambiguous-protocol-member]
        self.c: int  # error: [ambiguous-protocol-member]

    def non_init_method(self) -> None:
        self.x: int = 64  # fine
        self.y = "bar"  # fine

        self.d = 72  # error: [ambiguous-protocol-member]

# Note: the list of members does not include `a`, `b`, `c` or `d`,
# as none of these attributes is declared in the class body.
reveal_type(get_protocol_members(Foo))  # revealed: frozenset[Literal["non_init_method", "x", "y"]]
```

An explicit `Any` annotation on `self` does not change the object that Python passes to the method:

```py
class AnySelf(Protocol):
    def method(self: Any) -> None:
        self.attribute = 1  # error: [ambiguous-protocol-member]
```

Assignments in a comprehension and augmented assignments are also writes to the instance.
`__getattr__` provides the read side of `+=` below, so that case tests only the write:

```py
class AssignmentForms(Protocol):
    def __getattr__(self, name: str) -> int:
        return 0

    def comprehension(self) -> None:
        [None for self.from_comprehension in [1]]  # error: [ambiguous-protocol-member]

    def augmented_assignment(self) -> None:
        self.augmented += 1  # snapshot: ambiguous-protocol-member
```

```snapshot
warning[ambiguous-protocol-member]: Cannot assign to an undeclared attribute in a protocol method
   --> src/mdtest_snippet.py:326:9
    |
326 |         self.augmented += 1  # snapshot: ambiguous-protocol-member
    |         ^^^^^^^^^^^^^^ `augmented` is not declared as a protocol member
    |
info: Assigning to an undeclared attribute in a protocol method leads to an ambiguous interface
   --> src/mdtest_snippet.py:318:7
    |
318 | class AssignmentForms(Protocol):
    |       ^^^^^^^^^^^^^^^^^^^^^^^^^ `AssignmentForms` declared as a protocol here
    |
info: No declarations found for `augmented` in the body of `AssignmentForms` or any of its superclasses
```

If a member is declared in a superclass of a protocol class, the subclass can assign to it in the
class body or in a method without redeclaring it:

```py
class Super(Protocol):
    x: int

class Sub(Super, Protocol):
    x = 42  # no error here, since it's declared in the superclass

    def __init__(self) -> None:
        self.x = 43  # no error here either

reveal_type(get_protocol_members(Super))  # revealed: frozenset[Literal["x"]]
reveal_type(get_protocol_members(Sub))  # revealed: frozenset[Literal["x"]]
```

Assignments through an instance method's `self` parameter or a classmethod's `cls` parameter can
trigger this diagnostic. Static methods have no implicit receiver, while other parameters and
methods on concrete subclasses do not affect a protocol's declared interface:

```py
class Holder:
    extra: int

class WithStaticMethod(Protocol):
    @staticmethod
    def method(value: Holder) -> None:
        value.extra = 1  # no error

class WithClassMethod(Protocol):
    @classmethod
    def method(cls: Any) -> None:
        cls.extra = 1  # error: [ambiguous-protocol-member]

class WithDeclaredClassVariable(Protocol):
    extra: ClassVar[int]

    @classmethod
    def method(cls: Any) -> None:
        cls.extra = 1  # no error

class WithOtherParameter(Protocol):
    def method(self, value: Holder) -> None:
        value.extra = 1  # no error

class ConcreteSubclass(Foo):
    def method(self) -> None:
        self.extra = 1  # no error
```

Assignments can also occur in scopes nested inside a method. A nested class body or function that
uses the method's `self` still writes to the protocol instance, and a nested function can similarly
capture a classmethod's `cls`. An inner parameter named `self` refers to another object and is not
reported:

```py
class NestedScopes(Protocol):
    def class_body(self) -> None:
        class Nested:
            self.extra = 1  # error: [ambiguous-protocol-member]

    def function(self: Any) -> None:
        def inner() -> None:
            self.extra = 1  # error: [ambiguous-protocol-member]

        inner()

    def shadowed(self) -> None:
        def inner(self: Holder) -> None:
            self.extra = 1  # no error

        inner(Holder())

    @classmethod
    def class_method(cls: Any) -> None:
        def inner() -> None:
            cls.extra = 1  # error: [ambiguous-protocol-member]

        inner()
```

The runtime list of protocol members omits some names, including `__doc__`. An explicit declaration
still permits assignment to the attribute:

```py
class WithExcludedMember(Protocol):
    __doc__: str

    def method(self) -> None:
        self.__doc__ = "Protocol documentation"  # no error
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
from ty_extensions._internal import is_equivalent_to

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
from typing import Hashable, Protocol

class SupportsHash(Protocol):
    def __hash__(self) -> int: ...

static_assert(is_equivalent_to(object, Hashable))
static_assert(is_assignable_to(object, Hashable))
static_assert(is_subtype_of(object, Hashable))

def check_object_or_hashable(x: object | Hashable):
    reveal_type(x)  # revealed: object

def check_hashable_or_object(x: Hashable | object):
    reveal_type(x)  # revealed: object

def check_hashable_or_supports_hash(x: Hashable | SupportsHash):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_universal(x: Hashable | UniversalSet):
    reveal_type(x)  # revealed: Hashable
```

This means that any type considered assignable to `object` (which is all types) is considered by ty
to be assignable to `Hashable`. However, ty preserves a non-final nominal type in a union with
`Hashable` instead of discarding it as redundant. A non-final class can have unhashable subclasses,
so keeping the corresponding union element retains the annotation's more precise description of
those subclasses. For example, `list[str]` is unhashable but is a subtype of `Sequence[Hashable]`:

```py
from collections.abc import Hashable as AbcHashable
from typing import Sequence
from ty_extensions._internal import is_disjoint_from

def takes_hashable_or_sequence(x: Hashable | list[Hashable]): ...
def check_hashable_or_sequence(x: Hashable | Sequence[Hashable]):
    reveal_type(x)  # revealed: Hashable | Sequence[Hashable]

def check_abc_hashable_or_sequence(x: AbcHashable | Sequence[AbcHashable]):
    reveal_type(x)  # revealed: Hashable | Sequence[Hashable]

takes_hashable_or_sequence(["foo"])  # fine
takes_hashable_or_sequence(None)  # fine

static_assert(not is_disjoint_from(list[str], Hashable | list[Hashable]))
static_assert(not is_disjoint_from(list[str], Sequence[Hashable]))

static_assert(is_subtype_of(list[Hashable], Sequence[Hashable]))
static_assert(is_subtype_of(list[str], Sequence[Hashable]))
```

The additional union element is still simplified if it is a final class, because instances of the
class cannot override their inherited hashability:

```py
from dataclasses import dataclass
from typing import final

@final
class C: ...

@final
class Unhashable:
    __hash__: None = None

@final
class EqOnly:
    def __eq__(self, other: object, /) -> bool:
        return False

class EqOnlyBase:
    def __eq__(self, other: object, /) -> bool:
        return False

@final
class EqOnlyChild(EqOnlyBase): ...

@final
@dataclass
class UnhashableDataclass: ...

def check_hashable_or_final(x: Hashable | C):
    reveal_type(x)  # revealed: Hashable

# TODO: Preserve final classes that are known to be unhashable.
def check_hashable_or_unhashable_final(x: Hashable | Unhashable):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_eq_only(x: Hashable | EqOnly):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_eq_only_child(x: Hashable | EqOnlyChild):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_unhashable_dataclass(x: Hashable | UnhashableDataclass):
    reveal_type(x)  # revealed: Hashable
```

The special case is currently limited to nominal instance types:

```py
from typing import TypeVar, TypedDict

T = TypeVar("T")

class Payload(TypedDict):
    value: int

# TODO: Preserve non-nominal types that can contain unhashable values.
def check_hashable_or_typevar(x: Hashable | T):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_typed_dict(x: Hashable | Payload):
    reveal_type(x)  # revealed: Hashable

def check_hashable_or_protocol(x: Hashable | HasX):
    reveal_type(x)  # revealed: Hashable
```

We do not detect errors in cases like the following, which are flagged by other type checkers:

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
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to

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
static_assert(not is_equivalent_to(GenericProto, GenericProto[int]))  # error: [missing-type-argument]
static_assert(not is_equivalent_to(LegacyGenericProto, LegacyGenericProto[int]))  # error: [missing-type-argument]
```

## Intersections of protocols

An intersection of two protocol types `X` and `Y` is equivalent to a protocol type `Z` that inherits
from both `X` and `Y`:

```py
from typing import Protocol
from ty_extensions import Intersection, static_assert
from ty_extensions._internal import is_equivalent_to

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
from ty_extensions._internal import is_disjoint_from

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

Method members establish disjointness when their non-`Never` return types are disjoint. This is a
pragmatic approximation: strictly speaking, an implementation returning `Never` could satisfy method
signatures with otherwise disjoint return types.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal, Protocol
from typing_extensions import Never
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_disjoint_from, is_subtype_of

class HasLengthTwo(Protocol):
    def __len__(self) -> Literal[2]: ...

class LengthThree:
    def __len__(self) -> Literal[3]:
        return 3

class NeverLengthSubclass(LengthThree):
    def __len__(self) -> Never:
        raise RuntimeError

static_assert(is_subtype_of(NeverLengthSubclass, LengthThree))
static_assert(is_subtype_of(NeverLengthSubclass, HasLengthTwo))

# Intentionally unsound: `NeverLengthSubclass` inhabits both operands,
# but pragmatically, nobody is ever likely to write such a class
static_assert(is_disjoint_from(LengthThree, HasLengthTwo))
```

The same pragmatic approximation applies to fixed-length tuple types. A tuple subclass with a
`Never`-returning override demonstrates that the disjointness assertion here is also intentionally
unsound:

```py
class NeverLengthTupleSubclass(tuple[int, int, int]):
    def __len__(self) -> Never:
        raise RuntimeError

static_assert(is_subtype_of(NeverLengthTupleSubclass, tuple[int, int, int]))
static_assert(is_subtype_of(NeverLengthTupleSubclass, HasLengthTwo))

# Intentionally unsound: `NeverLengthTupleSubclass` inhabits both operands.
static_assert(is_disjoint_from(tuple[int, int, int], HasLengthTwo))
static_assert(not is_disjoint_from(tuple[int, int], HasLengthTwo))
```

Methods returning `Never` directly cannot establish this pragmatic disjointness. The same applies
when the return type is a type alias that resolves to `Never`:

```py
class NeverLength:
    def __len__(self) -> Never:
        raise RuntimeError

static_assert(not is_disjoint_from(NeverLength, HasLengthTwo))

type Bottom = Never

class AliasedNeverLength:
    def __len__(self) -> Bottom:
        raise RuntimeError

static_assert(is_assignable_to(AliasedNeverLength, HasLengthTwo))
static_assert(not is_disjoint_from(AliasedNeverLength, HasLengthTwo))
```

For overloaded methods, every possible return type on one side must be disjoint from every possible
return type on the other. A `Never` return in either overload set prevents the method from
establishing disjointness.

```py
from typing import Literal, Protocol, overload
from typing_extensions import Never
from ty_extensions import static_assert
from ty_extensions._internal import is_disjoint_from

class ReturnsOneOrTwo(Protocol):
    @overload
    def value(self, flag: Literal[True], /) -> Literal[1]: ...
    @overload
    def value(self, flag: Literal[False], /) -> Literal[2]: ...

class ReturnsThreeOrFour:
    @overload
    def value(self, flag: Literal[True], /) -> Literal[3]: ...
    @overload
    def value(self, flag: Literal[False], /) -> Literal[4]: ...
    def value(self, flag: bool, /) -> Literal[3, 4]:
        return 3 if flag else 4

class ReturnsTwoOrThree:
    @overload
    def value(self, flag: Literal[True], /) -> Literal[2]: ...
    @overload
    def value(self, flag: Literal[False], /) -> Literal[3]: ...
    def value(self, flag: bool, /) -> Literal[2, 3]:
        return 2 if flag else 3

static_assert(is_disjoint_from(ReturnsOneOrTwo, ReturnsThreeOrFour))
static_assert(not is_disjoint_from(ReturnsOneOrTwo, ReturnsTwoOrThree))

class ReturnsOneOrNever(Protocol):
    @overload
    def value(self, flag: Literal[True], /) -> Literal[1]: ...
    @overload
    def value(self, flag: Literal[False], /) -> Never: ...

class ReturnsThreeOrNever:
    @overload
    def value(self, flag: Literal[True], /) -> Literal[3]: ...
    @overload
    def value(self, flag: Literal[False], /) -> Never: ...
    def value(self, flag: bool, /) -> Literal[3]:
        return 3

static_assert(not is_disjoint_from(ReturnsOneOrNever, ReturnsThreeOrFour))
static_assert(not is_disjoint_from(ReturnsOneOrTwo, ReturnsThreeOrNever))

type BottomReturn = Never

class ReturnsThreeOrBottom:
    @overload
    def value(self, flag: Literal[True], /) -> Literal[3]: ...
    @overload
    def value(self, flag: Literal[False], /) -> BottomReturn: ...
    def value(self, flag: bool, /) -> Literal[3]:
        return 3

static_assert(not is_disjoint_from(ReturnsOneOrTwo, ReturnsThreeOrBottom))

class ReceiverFiltered[T]:
    payload: T

    @overload
    def value(self: "ReceiverFiltered[bytes]", flag: bool, /) -> bytes: ...
    @overload
    def value(self: "ReceiverFiltered[str]", flag: bool, /) -> str: ...
    def value(self, flag: bool, /) -> str | bytes:
        return ""

def empty_overloads(receiver: ReceiverFiltered[int]) -> None:
    reveal_type(receiver.value)  # revealed: Overload[]

static_assert(not is_disjoint_from(ReturnsOneOrTwo, ReceiverFiltered[int]))
```

## Intersections of protocols with types that have possibly unbound attributes

Note that if a `@final` class has a possibly unbound attribute corresponding to the protocol member,
instance types and class-literal types referring to that class cannot be a subtype of the protocol
but will also not be disjoint from the protocol:

`a.py`:

```py
from typing import final, ClassVar, Protocol
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of, is_disjoint_from, is_assignable_to

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
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of, is_disjoint_from, is_assignable_to

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
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_disjoint_from

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
others. A class object can satisfy the protocol through a declaration on its metaclass. Another
protocol can be a subtype of `HasX` either through "explicit" (nominal) inheritance from `HasX`, or
by specifying a superset of `HasX`'s interface:

`module.py`:

```py
x: int = 42
```

`main.py`:

```py
import module
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of, is_assignable_to

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

static_assert(is_subtype_of(UsesMeta, HasX))
static_assert(is_assignable_to(UsesMeta, HasX))
```

## `ClassVar` attribute members

If a protocol `ClassVarX` has a `ClassVar` attribute member `x` with type `int`, this indicates that
the non-callable attribute must be readable with the same type through both an inhabitant of
`ClassVarX` and the type of that inhabitant:

`classvars.py`:

```py
from typing import Any, ClassVar, Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

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

static_assert(not is_assignable_to(PropertyX, ClassVarXProto))
static_assert(not is_subtype_of(PropertyX, ClassVarXProto))

class ClassVarX:
    x: ClassVar[int] = 42

static_assert(is_assignable_to(ClassVarX, ClassVarXProto))
static_assert(is_subtype_of(ClassVarX, ClassVarXProto))

class XMeta(type):
    def x(cls) -> str:
        return ""

class ClassVarXWithConflictingMetaclass(metaclass=XMeta):
    x: ClassVar[int] = 42

static_assert(is_assignable_to(ClassVarXWithConflictingMetaclass, ClassVarXProto))
static_assert(is_subtype_of(ClassVarXWithConflictingMetaclass, ClassVarXProto))

class GenericMeta(type):
    x: list[Any] = []

class ClassVarXWithGenericMetaclass(metaclass=GenericMeta):
    x: ClassVar[int] = 42

static_assert(is_assignable_to(ClassVarXWithGenericMetaclass, ClassVarXProto))
static_assert(is_subtype_of(ClassVarXWithGenericMetaclass, ClassVarXProto))

# A class-level attribute shadows a non-data descriptor on the metaclass. In particular,
# `NotHashable.__hash__` takes precedence over the non-data `type.__hash__` descriptor.
class NotHashableProto(Protocol):
    __hash__: ClassVar[None]

class NotHashable:
    __hash__: ClassVar[None] = None

static_assert(is_assignable_to(NotHashable, NotHashableProto))
static_assert(is_subtype_of(NotHashable, NotHashableProto))
```

This is mentioned by the
[spec](https://typing.python.org/en/latest/spec/protocol.html#protocol-members) and tested in the
[conformance suite](https://github.com/python/typing/blob/main/conformance/tests/protocols_definition.py)
as something that must be supported by type checkers:

> To distinguish between protocol class variables and protocol instance variables, the special
> `ClassVar` annotation should be used.

## Declared instance attribute members

Declared protocol instance attributes should be available both on protocol-typed values and through
`self` inside protocol methods, with `Self` rebinding appropriately.

```py
from typing import Protocol
from typing_extensions import Self

class Linked(Protocol):
    value: int
    next: Self

    def advance(self) -> Self:
        reveal_type(self.value)  # revealed: int
        reveal_type(self.next)  # revealed: Self@advance
        return self.next

def f(x: Linked) -> None:
    reveal_type(x.value)  # revealed: int
    reveal_type(x.next)  # revealed: Linked
```

## Subtyping of protocols with property members

A read-only property on a protocol can be satisfied by a mutable attribute, a read-only property, a
read/write property, a `Final` attribute, or a `ClassVar` attribute:

```py
from typing import ClassVar, Final, Protocol, final
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to, is_disjoint_from

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

@final
class FinalXReadProperty:
    @property
    def x(self) -> int:
        return 42

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

static_assert(not is_assignable_to(XAttrBad, HasXProperty))
static_assert(not is_assignable_to(HasStrXProperty, HasXProperty))
static_assert(not is_assignable_to(HasXProperty, HasStrXProperty))
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
    def x(self) -> MyInt: ...

static_assert(is_subtype_of(XSubProto, HasXProperty))
static_assert(is_assignable_to(XSubProto, HasXProperty))
```

A `Final` attribute on a protocol is also read-only:

```py
class HasFinalX(Protocol):
    # A Final protocol member is an instance declaration and does not require a value.
    x: Final[int]

class HasFinalClassVarX(Protocol):
    # The ClassVar qualifier is meaningful in a protocol and should not trigger
    # redundant-final-classvar.
    x: ClassVar[Final[int]]

static_assert(is_subtype_of(XFinal, HasFinalX))
static_assert(is_assignable_to(XFinal, HasFinalX))
static_assert(is_subtype_of(XReadProperty, HasFinalX))
static_assert(is_assignable_to(XReadProperty, HasFinalX))
static_assert(is_subtype_of(HasXProperty, HasFinalX))
static_assert(is_assignable_to(HasXProperty, HasFinalX))
static_assert(is_subtype_of(HasFinalClassVarX, HasFinalX))
static_assert(is_assignable_to(HasFinalClassVarX, HasFinalX))
static_assert(not is_subtype_of(HasFinalX, HasFinalClassVarX))
static_assert(not is_assignable_to(HasFinalX, HasFinalClassVarX))
static_assert(not is_subtype_of(XReadProperty, HasFinalClassVarX))
static_assert(not is_assignable_to(XReadProperty, HasFinalClassVarX))

class MutableClassVarX:
    x: int = 0

class FinalClassVarImplementation:
    x: Final[int] = 0

static_assert(is_subtype_of(MutableClassVarX, HasFinalClassVarX))
static_assert(is_assignable_to(MutableClassVarX, HasFinalClassVarX))
static_assert(is_subtype_of(FinalClassVarImplementation, HasFinalClassVarX))
static_assert(is_assignable_to(FinalClassVarImplementation, HasFinalClassVarX))
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

static_assert(not is_subtype_of(XReadProperty, HasMutableXProperty))
static_assert(not is_assignable_to(XReadProperty, HasMutableXProperty))

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

static_assert(not is_subtype_of(XSub, HasMutableXProperty))
static_assert(not is_assignable_to(XSub, HasMutableXProperty))
```

A protocol with a read/write property `x` is exactly equivalent to a protocol with a mutable
attribute `x`. Both are subtypes of a protocol with a read-only property `x`:

```py
from ty_extensions._internal import is_equivalent_to

class HasMutableXAttr(Protocol):
    x: int

static_assert(is_equivalent_to(HasMutableXAttr, HasMutableXProperty))
static_assert(not is_disjoint_from(FinalXReadProperty, HasXProperty))
static_assert(is_disjoint_from(FinalXReadProperty, HasMutableXAttr))
static_assert(not is_subtype_of(HasFinalX, HasMutableXAttr))
static_assert(not is_assignable_to(HasFinalX, HasMutableXAttr))

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

static_assert(not is_assignable_to(HasMutableXAttrWrongType, HasXProperty))
static_assert(not is_assignable_to(HasMutableXAttrWrongType, HasMutableXProperty))
static_assert(not is_assignable_to(HasMutableXProperty, HasMutableXAttrWrongType))
```

Literal values use their fallback instance type when checking writable property requirements:

```py
class JustInt(Protocol):
    @property
    def __class__(self) -> type[int]: ...
    @__class__.setter
    def __class__(self, value: type[int]) -> None: ...

int_value: JustInt = 1
bool_value: JustInt = True  # error: [invalid-assignment]
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

static_assert(is_subtype_of(XAsymmetricProperty, HasAsymmetricXProperty))
static_assert(is_assignable_to(XAsymmetricProperty, HasAsymmetricXProperty))

from typing import Any

class ObjectReadAnyWriteProperty:
    @property
    def x(self) -> object:
        return object()

    @x.setter
    def x(self, value: Any) -> None: ...

class HasObjectReadIntWriteProperty(Protocol):
    @property
    def x(self) -> object: ...
    @x.setter
    def x(self, value: int) -> None: ...

static_assert(not is_subtype_of(ObjectReadAnyWriteProperty, HasObjectReadIntWriteProperty))
static_assert(is_assignable_to(ObjectReadAnyWriteProperty, HasObjectReadIntWriteProperty))
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

from typing import overload

class HasIntOrStrWriteProperty(Protocol):
    @property
    def x(self) -> object: ...
    @x.setter
    def x(self, value: int | str) -> None: ...

class OverloadedSetterDescriptor:
    def __get__(self, instance, owner) -> object:
        return object()

    @overload
    def __set__(self, instance, value: int) -> None: ...
    @overload
    def __set__(self, instance, value: str) -> None: ...
    def __set__(self, instance, value: int | str) -> None: ...

class ObjectReadOverloadedWriteDescriptor:
    x: OverloadedSetterDescriptor = OverloadedSetterDescriptor()

static_assert(is_subtype_of(ObjectReadOverloadedWriteDescriptor, HasIntOrStrWriteProperty))
static_assert(is_assignable_to(ObjectReadOverloadedWriteDescriptor, HasIntOrStrWriteProperty))

class AnySetterDescriptor:
    def __get__(self, instance, owner) -> object:
        return object()

    def __set__(self, instance, value: Any) -> None: ...

class ObjectReadAnyWriteDescriptor:
    x: AnySetterDescriptor = AnySetterDescriptor()

static_assert(not is_subtype_of(ObjectReadAnyWriteDescriptor, HasObjectReadIntWriteProperty))
static_assert(is_assignable_to(ObjectReadAnyWriteDescriptor, HasObjectReadIntWriteProperty))
```

A property's setter return type does not affect whether it satisfies a writable protocol member.
Ordinary assignment still reports an error if the setter never returns:

```py
from typing_extensions import Never

class TerminalPropertySetter:
    @property
    def x(self) -> int:
        return 1

    @x.setter
    def x(self, value: int) -> Never:
        raise RuntimeError

static_assert(is_subtype_of(TerminalPropertySetter, HasMutableXProperty))
static_assert(is_assignable_to(TerminalPropertySetter, HasMutableXProperty))

terminal_property = TerminalPropertySetter()
terminal_property.x = 1  # error: [invalid-assignment]
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

static_assert(is_subtype_of(HasGetAttrAndSetAttr, HasXProperty))
static_assert(is_assignable_to(HasGetAttrAndSetAttr, HasXProperty))

class HasGetAttrAndAnySetAttr:
    def __getattr__(self, attr: str) -> object:
        return object()

    def __setattr__(self, attr: str, value: Any) -> None: ...

static_assert(not is_subtype_of(HasGetAttrAndAnySetAttr, HasObjectReadIntWriteProperty))
static_assert(is_assignable_to(HasGetAttrAndAnySetAttr, HasObjectReadIntWriteProperty))

# TODO: these should pass
static_assert(is_subtype_of(HasGetAttrAndSetAttr, XAsymmetricProperty))  # error: [static-assert-error]
static_assert(is_assignable_to(HasGetAttrAndSetAttr, XAsymmetricProperty))  # error: [static-assert-error]

class HasSetAttrWithUnsuitableInput:
    def __getattr__(self, attr: str) -> int:
        return 1

    def __setattr__(self, attr: str, value: str) -> None: ...

static_assert(not is_subtype_of(HasSetAttrWithUnsuitableInput, HasMutableXProperty))
static_assert(not is_assignable_to(HasSetAttrWithUnsuitableInput, HasMutableXProperty))

# For static checking, an explicit attribute declaration takes precedence over `__setattr__`.
# This matches other type checkers and likely user intent, even though a custom `__setattr__`
# intercepts every assignment at runtime.
class ExplicitXWithBroadSetAttr:
    x: int

    def __setattr__(self, attr: str, value: object) -> None: ...

class HasStringSetter(Protocol):
    @property
    def x(self) -> int: ...
    @x.setter
    def x(self, value: str) -> None: ...

static_assert(not is_subtype_of(ExplicitXWithBroadSetAttr, HasStringSetter))
static_assert(not is_assignable_to(ExplicitXWithBroadSetAttr, HasStringSetter))

explicit_x = ExplicitXWithBroadSetAttr()
explicit_x.x = "string"  # error: [invalid-assignment]
```

Writable attributes annotated with `Self` are checked after binding `Self` to the implementation
type:

```py
from typing_extensions import Self

class WritableSelfAttr:
    x: Self

class RecursiveWritableSelfAttr(Protocol):
    x: Self

# TODO: Add an equivalent property protocol and an `is_equivalent_to` assertion once `Self` types
# are supported in protocol member comparisons.
class HasWritableSelfAttr(Protocol):
    @property
    def x(self) -> WritableSelfAttr: ...
    @x.setter
    def x(self, value: WritableSelfAttr) -> None: ...

static_assert(is_subtype_of(WritableSelfAttr, HasWritableSelfAttr))
static_assert(is_assignable_to(WritableSelfAttr, HasWritableSelfAttr))

def _(value: WritableSelfAttr) -> None:
    value.x = WritableSelfAttr()

def assign_protocol_member(left: RecursiveWritableSelfAttr, right: RecursiveWritableSelfAttr) -> None:
    left.x = right
```

Property members annotated with `Self` bind it to the implementation type:

```py
class HasReadableSelfProperty(Protocol):
    @property
    def x(self) -> Self: ...

class ReadableSelfProperty:
    @property
    def x(self) -> "ReadableSelfProperty":
        return self

# TODO: These should pass once `Self` protocol members are checked against all possible subclasses
# of the implementation.
static_assert(not is_subtype_of(ReadableSelfProperty, HasReadableSelfProperty))  # error: [static-assert-error]
static_assert(not is_assignable_to(ReadableSelfProperty, HasReadableSelfProperty))  # error: [static-assert-error]

class HasWritableSelfProperty(Protocol):
    @property
    def x(self) -> object: ...
    @x.setter
    def x(self, value: Self) -> None: ...

class WritableSelfProperty:
    @property
    def x(self) -> "WritableSelfProperty":
        return self

    @x.setter
    def x(self, value: "WritableSelfProperty") -> None: ...

static_assert(is_subtype_of(WritableSelfProperty, HasWritableSelfProperty))
static_assert(is_assignable_to(WritableSelfProperty, HasWritableSelfProperty))

class PropertyWithSelfSetter:
    @property
    def x(self) -> object:
        return self

    @x.setter
    def x(self, value: Self) -> None: ...

class HasConcretePropertySetter(Protocol):
    @property
    def x(self) -> object: ...
    @x.setter
    def x(self, value: PropertyWithSelfSetter) -> None: ...

static_assert(is_subtype_of(PropertyWithSelfSetter, HasConcretePropertySetter))
static_assert(is_assignable_to(PropertyWithSelfSetter, HasConcretePropertySetter))
```

## Protocol members defined using descriptor decorators

### Descriptor reads and writes

On an instance, a protocol member defined using a descriptor decorator has the type returned by
`__get__`, not the type of the descriptor stored on the protocol class. If the descriptor defines
`__set__`, its value parameter determines which assignments are valid:

```py
from typing import Protocol

class StringDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> str:
        return "example"

    def __set__(self, instance: object, value: str) -> None: ...

class HasName(Protocol):
    @StringDescriptor
    def name(self) -> object: ...

class WithName:
    name: str = "example"

has_name: HasName = WithName()
reveal_type(has_name.name)  # revealed: str
has_name.name = "updated"
has_name.name = 1  # error: [invalid-assignment]
```

### `cached_property`

The standard-library `cached_property` descriptor uses the same behavior:

```py
from functools import cached_property
from typing import Protocol

class HasCachedName(Protocol):
    @cached_property
    def name(self) -> str: ...

class WithCachedName:
    @cached_property
    def name(self) -> str:
        return "example"

has_name: HasCachedName = WithCachedName()
```

### Generic descriptor result types

Applying a generic descriptor decorator to a generic protocol method currently loses the protocol's
type variable and produces `cached_property[Unknown]`. The protocol must preserve that descriptor
type instead of reducing it to a bare `Unknown`, which would allow an incompatible implementation.

```py
from functools import cached_property
from typing import Protocol, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, reveal_protocol_interface

T = TypeVar("T")

class HasValue(Protocol[T]):
    @cached_property
    def value(self) -> T: ...

class StrValue:
    @cached_property
    def value(self) -> str:
        return "value"

static_assert(not is_assignable_to(StrValue, HasValue[int]))

# TODO: This should be a property with an `int` read type once decorator calls preserve enclosing
# type variables.
# revealed: {"value": AttributeMember(`cached_property[Unknown]`)}
reveal_protocol_interface(HasValue[int])
```

### Descriptor values in annotations

Only a descriptor produced by decorating a protocol method changes how that member is read and
written through an instance. An annotation whose type implements the descriptor protocol still
declares an ordinary attribute whose protocol member type is the descriptor object. We reveal the
protocol interface here because ordinary instance access would invoke `cached_property.__get__` and
reveal `str` in both cases:

```py
from functools import cached_property
from typing import Protocol
from ty_extensions._internal import reveal_protocol_interface

class StoresDescriptor(Protocol):
    name: cached_property[str]

# revealed: {"name": AttributeMember(`cached_property[str]`)}
reveal_protocol_interface(StoresDescriptor)
```

### Overloaded setters selected by receiver type

An overloaded `__set__` method can accept different values for different receiver types. For
`HasValue`, the overloads with an `object` receiver accept `int` and `bytes`; the overload for
`Other` does not apply. Therefore, assignments of `int` and `bytes` are valid, but assignments of
`str` are not.

```py
from typing import Protocol, final, overload

@final
class Other: ...

class ReceiverSensitiveDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        raise NotImplementedError

    @overload
    def __set__(self, instance: object, value: int) -> None: ...
    @overload
    def __set__(self, instance: object, value: bytes) -> None: ...
    @overload
    def __set__(self, instance: Other, value: str) -> None: ...
    def __set__(self, instance: object, value: int | bytes | str) -> None: ...

class HasValue(Protocol):
    @ReceiverSensitiveDescriptor
    def value(self) -> int: ...

class ReadOnlyValue:
    @property
    def value(self) -> int:
        return 1

read_only: HasValue = ReadOnlyValue()  # error: [invalid-assignment]

def update_value(value: HasValue) -> None:
    value.value = 1
    value.value = b"valid"
    value.value = "bad"  # error: [invalid-assignment]
```

### Descriptor setters on union protocol receivers

When assigning through a union of protocols, each descriptor setter is called with its matching
union element as the receiver. The other elements of the union do not participate in that call. The
`a_only` and `b_only` methods keep the two protocol types distinct.

```py
from __future__ import annotations

from typing import Protocol

class ADescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: A, value: int) -> None: ...

class BDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: B, value: int) -> None: ...

class A(Protocol):
    @ADescriptor
    def value(self) -> int: ...
    def a_only(self) -> None: ...

class B(Protocol):
    @BDescriptor
    def value(self) -> int: ...
    def b_only(self) -> None: ...

def update_union_value(value: A | B) -> None:
    value.value = 1
```

### Static, class, and callable setters

The examples below use the same property implementations to check both assignment and protocol
compatibility:

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class IntPropertySetter:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, new_value: int) -> None: ...

class StrPropertySetter:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, new_value: str) -> None: ...
```

A static `__set__` method receives the instance and assigned value directly:

```py
class StaticSetterDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    @staticmethod
    def __set__(instance: object, value: int) -> None: ...

class HasStaticSetter(Protocol):
    @StaticSetterDescriptor
    def value(self) -> int: ...

static_assert(is_subtype_of(IntPropertySetter, HasStaticSetter))
static_assert(not is_subtype_of(StrPropertySetter, HasStaticSetter))

def update_static_setter(value: HasStaticSetter) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

A class `__set__` method also receives the descriptor class implicitly:

```py
class ClassSetterDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    @classmethod
    def __set__(cls, instance: object, value: int) -> None: ...

class HasClassSetter(Protocol):
    @ClassSetterDescriptor
    def value(self) -> int: ...

static_assert(is_subtype_of(IntPropertySetter, HasClassSetter))
static_assert(not is_subtype_of(StrPropertySetter, HasClassSetter))

def update_class_setter(value: HasClassSetter) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

An object stored in `__set__` is called with the instance and assigned value:

```py
class IntSetterCallable:
    def __call__(self, instance: object, value: int) -> None: ...

class CallableSetterDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    __set__ = IntSetterCallable()

class HasCallableSetter(Protocol):
    @CallableSetterDescriptor
    def value(self) -> int: ...

static_assert(is_subtype_of(IntPropertySetter, HasCallableSetter))
static_assert(not is_subtype_of(StrPropertySetter, HasCallableSetter))

def update_callable_setter(value: HasCallableSetter) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

### Union descriptor types

If a decorator can return either of two descriptors, an assignment must be accepted by both possible
descriptors. Here, only `str` is accepted by both, so an `int` assignment is invalid even though one
of the descriptors accepts it.

```py
from typing import Generic, Protocol, TypeVar

T = TypeVar("T")

class Descriptor(Generic[T]):
    def __get__(self, instance: object, owner: type | None = None) -> T:
        raise NotImplementedError

    def __set__(self, instance: object, value: T) -> None: ...

def either_descriptor(getter: object) -> Descriptor[int | str] | Descriptor[str | bytes]:
    raise NotImplementedError

class HasEitherValue(Protocol):
    @either_descriptor
    def either_value(self) -> object: ...

def update_either_value(value: HasEitherValue) -> None:
    value.either_value = "valid"
    value.either_value = 1  # error: [invalid-assignment]
```

### Aliased union descriptor types

Top-level PEP 695 aliases do not change which assignments a descriptor union accepts. As with the
unaliased form above, only `str` is accepted by both possible descriptors. The alias also does not
make the protocol member read-only.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, Protocol, TypeVar

T = TypeVar("T")

class Descriptor(Generic[T]):
    def __get__(self, instance: object, owner: type | None = None) -> T:
        raise NotImplementedError

    def __set__(self, instance: object, value: T) -> None: ...

type DescriptorAlias = Descriptor[int | str] | Descriptor[str | bytes]

def aliased_descriptor(getter: object) -> DescriptorAlias:
    raise NotImplementedError

class HasAliasedValue(Protocol):
    @aliased_descriptor
    def value(self) -> object: ...

class ReadOnlyAliasedValue:
    @property
    def value(self) -> object:
        return "value"

read_only: HasAliasedValue = ReadOnlyAliasedValue()  # error: [invalid-assignment]

def update_aliased_value(value: HasAliasedValue) -> None:
    value.value = "valid"
    value.value = 1  # error: [invalid-assignment]
```

### Large unions of descriptor types

A value assigned through the protocol must be accepted by every possible descriptor. Here, `AX` is
accepted by both descriptors, while `A` is accepted only by the first. Because the protocol member
is writable, a read-only property cannot implement it.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, Protocol, TypeVar

T = TypeVar("T")

class A: ...
class B: ...
class C: ...
class X: ...
class Y: ...
class Z: ...
class AX(A, X): ...

class Descriptor(Generic[T]):
    def __get__(self, instance: object, owner: type | None = None) -> object:
        raise NotImplementedError

    def __set__(self, instance: object, value: T) -> None: ...

def large_union_descriptor(
    getter: object,
) -> Descriptor[A | B | C] | Descriptor[X | Y | Z]:
    raise NotImplementedError

class HasLargeUnionValue(Protocol):
    @large_union_descriptor
    def value(self) -> object: ...

class ReadOnlyLargeUnionValue:
    @property
    def value(self) -> object:
        return "value"

read_only: HasLargeUnionValue = ReadOnlyLargeUnionValue()  # error: [invalid-assignment]

def update_large_union_value(
    value: HasLargeUnionValue,
    valid: AX,
    invalid: A,
) -> None:
    value.value = valid
    value.value = invalid  # error: [invalid-assignment]
```

### Overloaded setters selected by descriptor type

An overload can also restrict the type of the descriptor itself. The decorator below returns
`SelfSensitiveDescriptor[int]`, so only the overload accepting an `int` value applies.

```py
from __future__ import annotations

from typing import Generic, Protocol, TypeVar, overload

T = TypeVar("T")

class SelfSensitiveDescriptor(Generic[T]):
    def __get__(self, instance: object, owner: type | None = None) -> T:
        raise NotImplementedError

    @overload
    def __set__(self: SelfSensitiveDescriptor[int], instance: object, value: int) -> None: ...
    @overload
    def __set__(self: SelfSensitiveDescriptor[str], instance: object, value: str) -> None: ...
    def __set__(self, instance: object, value: int | str) -> None: ...

def int_descriptor(getter: object) -> SelfSensitiveDescriptor[int]:
    raise NotImplementedError

class HasIntValue(Protocol):
    @int_descriptor
    def int_value(self) -> int: ...

def update_int_value(value: HasIntValue) -> None:
    value.int_value = 1
    value.int_value = "bad"  # error: [invalid-assignment]
```

### Generic setter value types

A setter that uses a method type variable directly as its value parameter accepts every value
allowed by that type variable's upper bound. The setter below therefore accepts `int` values.

```py
from typing import Protocol, TypeVar

T = TypeVar("T", bound=int)

class BoundedDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: object, value: T) -> None: ...

def bounded_descriptor(getter: object) -> BoundedDescriptor:
    raise NotImplementedError

class HasBoundedValue(Protocol):
    @bounded_descriptor
    def bounded_value(self) -> int: ...

def update_bounded_value(value: HasBoundedValue) -> None:
    value.bounded_value = 1
    value.bounded_value = "bad"  # error: [invalid-assignment]
```

### Type variables from the surrounding function

A type variable supplied by the surrounding function is still the descriptor's value type. Assigning
a value of that type is valid.

```py
from typing import Generic, Protocol, TypeVar

T = TypeVar("T")
U = TypeVar("U")

class Descriptor(Generic[T]):
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> T:
        raise NotImplementedError

    def __set__(self, instance: object, value: T) -> None: ...

class HasGenericValue(Protocol[T]):
    @Descriptor[T]
    def value(self) -> T: ...

def update_generic_value(value: HasGenericValue[U], new_value: U) -> None:
    value.value = new_value
```

### Setter type variables inside aliases

A type alias does not hide that `T` belongs to `__set__`. The descriptor below still accepts `int`,
so it remains writable with `int` but not with `str`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Never, Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

type Alias[T] = T

class AliasedReceiverDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__[T](self, instance: Alias[T], value: int) -> None: ...

class HasAliasedReceiver(Protocol):
    @AliasedReceiverDescriptor
    def value(self) -> int: ...
```

A property setter that accepts only `Never` cannot implement this protocol member:

```py
class NeverPropertySetter:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, new_value: Never) -> None: ...

static_assert(not is_subtype_of(NeverPropertySetter, HasAliasedReceiver))
```

Assignments through the protocol accept `int` but reject `str`:

```py
def update_aliased_receiver(value: HasAliasedReceiver) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

### Constrained generic setters

A setter with a constrained type variable must choose one constraint for each call. It accepts `int`
and `str` separately, but not a value whose type is `int | str`.

```py
from typing import Protocol, TypeVar

T = TypeVar("T", int, str)

class ConstrainedDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int | str:
        raise NotImplementedError

    def __set__(self, instance: object, value: T) -> None: ...

def constrained_descriptor(getter: object) -> ConstrainedDescriptor:
    raise NotImplementedError

class HasConstrainedValue(Protocol):
    @constrained_descriptor
    def constrained_value(self) -> int | str: ...
```

The setter is still present, so a read-only property cannot implement the protocol:

```py
class ReadOnlyConstrainedValue:
    @property
    def constrained_value(self) -> int | str:
        return "value"

read_only: HasConstrainedValue = ReadOnlyConstrainedValue()  # error: [invalid-assignment]
```

Literal values select one constraint, while a union of the constraints does not:

```py
def update_constrained_value(value: HasConstrainedValue, new_value: int | str) -> None:
    value.constrained_value = 1
    value.constrained_value = "valid"
    value.constrained_value = new_value  # error: [invalid-assignment]
```

### Setters that accept `Never`

A `Never` value cannot normally exist, but a `__set__` parameter of type `Never` still makes the
protocol member writable. A read-only property therefore cannot implement it.

```py
from typing import Protocol
from typing_extensions import Never

class NeverDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> object:
        raise NotImplementedError

    def __set__(self, instance: object, value: Never) -> None: ...

def never_descriptor(getter: object) -> NeverDescriptor:
    raise NotImplementedError

class HasNeverValue(Protocol):
    @never_descriptor
    def never_value(self) -> object: ...

class ReadOnlyNeverValue:
    @property
    def never_value(self) -> object:
        return "value"

read_only: HasNeverValue = ReadOnlyNeverValue()  # error: [invalid-assignment]
```

If the assigned expression itself has type `Never`, the assignment is valid:

```py
def update_never_value(value: HasNeverValue, new_value: Never) -> None:
    value.never_value = new_value
```

### Optional parameters after the setter value

Attribute assignment calls `__set__` with the instance and assigned value. Any later parameters can
be present if they can all be omitted.

```py
from typing import Protocol

class OptionalTrailingDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        raise NotImplementedError

    def __set__(
        self,
        instance: object,
        value: int,
        notify: bool = False,
        *metadata: str,
        log: bool = False,
        **named_metadata: str,
    ) -> None: ...

class HasOptionalTrailingValue(Protocol):
    @OptionalTrailingDescriptor
    def value(self) -> int: ...

def update_optional_trailing_value(value: HasOptionalTrailingValue) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

### Gradual variadic tails after the setter value

A `*args: Any, **kwargs: Any` tail can also be omitted. It does not make the protocol member
read-only or change the `int` value accepted by the setter.

```py
from typing import Any, Protocol

class GradualTrailingDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        raise NotImplementedError

    def __set__(self, instance: object, value: int, *args: Any, **kwargs: Any) -> None: ...

class HasGradualTrailingValue(Protocol):
    @GradualTrailingDescriptor
    def value(self) -> int: ...

class ReadOnlyGradualTrailingValue:
    @property
    def value(self) -> int:
        return 1

read_only: HasGradualTrailingValue = ReadOnlyGradualTrailingValue()  # error: [invalid-assignment]

def update_gradual_trailing_value(value: HasGradualTrailingValue) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

### Required parameters after the setter value

If `__set__` requires another parameter after the assigned value, attribute assignment cannot call
it because that argument is missing.

```py
from typing import Protocol

class RequiredTrailingDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        raise NotImplementedError

    def __set__(self, instance: object, value: int, required: bool) -> None: ...

class HasRequiredTrailingValue(Protocol):
    @RequiredTrailingDescriptor
    def value(self) -> int: ...

def update_required_trailing_value(value: HasRequiredTrailingValue) -> None:
    value.value = 1  # error: [invalid-assignment]
```

### Setter values captured by `*args`

When `__set__` declares `(instance, *values: int)`, attribute assignment supplies the value as the
first element of `values`. The descriptor therefore accepts `int` but not `str`.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class VariadicValueDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: object, *values: int) -> None: ...

class HasVariadicValue(Protocol):
    @VariadicValueDescriptor
    def value(self) -> int: ...
```

A property setter restricted to `str` cannot implement this protocol member:

```py
class StrPropertySetter:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, new_value: str) -> None: ...

static_assert(not is_subtype_of(StrPropertySetter, HasVariadicValue))
```

Assignments through the protocol follow the `int` annotation on `*values`:

```py
def update_variadic_value(value: HasVariadicValue) -> None:
    value.value = 1
    value.value = "bad"  # error: [invalid-assignment]
```

### Gradually typed setter signatures

When `__set__` is `Callable[..., None]`, ty cannot determine which values it accepts. Assignment is
allowed, but a property setter limited to `int` is not guaranteed to implement the same member.

```py
from typing import Any, Callable, Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

class IntPropertySetter:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, new_value: int) -> None: ...

class CallableSetterDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    __set__: Callable[..., None]

class HasCallableSetter(Protocol):
    @CallableSetterDescriptor
    def value(self) -> int: ...

static_assert(not is_subtype_of(IntPropertySetter, HasCallableSetter))

def update_callable_setter(value: HasCallableSetter) -> None:
    value.value = object()
```

The same applies when the type of `__set__` is `Any`:

```py
class AnySetterDescriptor:
    def __init__(self, getter: object) -> None: ...
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    __set__: Any

class HasAnySetter(Protocol):
    @AnySetterDescriptor
    def value(self) -> int: ...

static_assert(not is_subtype_of(IntPropertySetter, HasAnySetter))

def update_any_setter(value: HasAnySetter) -> None:
    value.value = object()
```

## Variance of generic protocols with `Final` members

A `Final` attribute is readable but not writable, so it constrains an inferred type parameter
covariantly:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Final, Protocol, cast
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class MyInt(int): ...

class GenericFinalX[T](Protocol):
    x: Final[T] = cast(T, None)

static_assert(is_subtype_of(GenericFinalX[MyInt], GenericFinalX[int]))
static_assert(is_assignable_to(GenericFinalX[MyInt], GenericFinalX[int]))
static_assert(not is_subtype_of(GenericFinalX[int], GenericFinalX[MyInt]))
static_assert(not is_assignable_to(GenericFinalX[int], GenericFinalX[MyInt]))
```

## Subtyping of protocols with method members

A protocol can have method members. `T` is assignable to `P` in the following example because the
class `T` has a method `m` which is assignable to the `Callable` supertype of the method `P.m`:

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

class P(Protocol):
    def m(self, x: int, /) -> None: ...

class PWithClassMethod(Protocol):
    @classmethod
    def m(cls, x: int, /) -> None: ...

class PWithStaticMethod(Protocol):
    @staticmethod
    def m(x: int, /) -> None: ...

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

class NominalWithStaticMethodGood:
    @staticmethod
    def m(x: int) -> None: ...

class DefinitelyNotSubtype:
    m = None

static_assert(is_subtype_of(NominalSubtype, P))
static_assert(is_subtype_of(NominalSubtype2, P))
static_assert(is_subtype_of(NominalSubtype | NominalSubtype2, P))
static_assert(not is_assignable_to(DefinitelyNotSubtype, P))
static_assert(not is_assignable_to(NotSubtype, P))
static_assert(not is_assignable_to(NominalSubtype | NotSubtype, P))
static_assert(not is_assignable_to(NominalSubtype2 | DefinitelyNotSubtype, P))

# A classmethod or staticmethod can satisfy a regular method member if it has the correct
# signature when accessed on an instance. The class-side check only establishes that the member
# is present on the class.
static_assert(is_assignable_to(NominalWithClassMethod, P))
static_assert(is_assignable_to(NominalWithStaticMethodGood, P))
static_assert(is_assignable_to(NominalSubtype | NominalWithClassMethod, P))
static_assert(is_assignable_to(NominalSubtype | NominalWithStaticMethodGood, P))
static_assert(is_subtype_of(PWithClassMethod, P))
static_assert(is_subtype_of(PWithStaticMethod, P))

# This staticmethod has an extra parameter when accessed on an instance.
static_assert(not is_assignable_to(NominalWithStaticMethod, P))
static_assert(not is_assignable_to(NominalSubtype | NominalWithStaticMethod, P))
```

A callable instance attribute is not sufficient for a type to satisfy a protocol with a method
member: a method member specified by a protocol `P` must exist on the *meta-type* of `T` for `T` to
be a subtype of `P`:

```py
from typing import Callable, Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to

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

The reason for this is that some methods, such as Python's special methods, are always looked up on
the class directly. If a class with an `__iter__` instance attribute satisfied the `Iterable`
protocol, for example, the `Iterable` protocol would not accurately describe the requirements Python
has for a class to be iterable at runtime. Allowing callable instance attributes to satisfy method
members of protocols would also make `issubclass()` narrowing of runtime-checkable protocols
unsound, as the `issubclass()` mechanism at runtime for protocols only checks whether a method is
accessible on the class object, not the instance. (Protocols with non-method members cannot be
passed to `issubclass()` at all at runtime.)

```py
from typing import Iterable, Any
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to

class Foo:
    def __init__(self):
        self.__iter__: Callable[..., object] = lambda *args, **kwargs: None

static_assert(not is_assignable_to(Foo, Iterable[Any]))
```

Enforcing that members must always be available on the class also means that it is safe to access a
method on `type[P]`, where `P` is a protocol class, just like it is generally safe to access a
method on `type[C]` where `C` is a nominal class:

```py
from typing import Protocol

class Foo(Protocol):
    def method(self) -> str: ...

def f(x: Foo):
    reveal_type(type(x).method)  # revealed: (self, /) -> str

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

Literal values should satisfy protocols with method members via their instance fallback type:

```py
from typing import Literal, Protocol, TypeVar

reveal_type(abs(5))  # revealed: int

def f(x: Literal[5]) -> None:
    reveal_type(abs(x))  # revealed: int

InT = TypeVar("InT")
OutT = TypeVar("OutT")

class CanMul(Protocol[InT, OutT]):
    def __mul__(self, x: InT, /) -> OutT: ...

def x2(x: CanMul[int, OutT], /) -> OutT:
    return x * 2

def g(x: int) -> None:
    reveal_type(x2(x))  # revealed: int

reveal_type(x2(1))  # revealed: int
```

The class-side check for a method member only establishes that the member is present. Its signature
is checked through the instance, so the class-side check must not add the same generic constraints a
second time. This matters when checking a covariant protocol that also has non-method members:

```py
from collections.abc import Iterator
from typing import Any, Protocol, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to

T_co = TypeVar("T_co", covariant=True)

class CovariantList(Protocol[T_co]):
    @property
    def __class__(self) -> type[list[Any]]: ...
    @__class__.setter
    def __class__(self, value: type[list[Any]], /) -> None: ...
    def __iter__(self) -> Iterator[T_co]: ...

static_assert(is_assignable_to(list[int], CovariantList[float]))
```

Protocol method return types can contain mutually recursive protocols. Reducing methods to their
instance and class access capabilities must preserve callable-specific cycle normalization:

```py
from collections.abc import Iterable
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of

class RichCast(Protocol):
    def __rich__(self) -> "ConsoleRenderable | RichCast": ...

class ConsoleRenderable(Protocol):
    def __rich_console__(self) -> "Iterable[ConsoleRenderable | RichCast | int]": ...

class Text:
    def __rich_console__(self) -> Iterable[int]:
        raise NotImplementedError

static_assert(is_subtype_of(Text, ConsoleRenderable))
static_assert(is_assignable_to(Text, ConsoleRenderable))
```

## Subtyping of protocols with generic method members

Protocol method members can be generic. They can have generic contexts scoped to the class:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, final, overload
from typing_extensions import TypeVar, Self, Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to, is_assignable_to, is_subtype_of

class NewStyleClassScoped[T](Protocol):
    def method(self, input: T) -> None: ...

S = TypeVar("S")

class LegacyClassScoped(Protocol[S]):
    def method(self, input: S) -> None: ...

# error: [missing-type-argument]
static_assert(is_equivalent_to(NewStyleClassScoped, LegacyClassScoped))  # error: [missing-type-argument]
static_assert(is_equivalent_to(NewStyleClassScoped[int], LegacyClassScoped[int]))

class NominalGeneric[T]:
    def method(self, input: T) -> None: ...

def _[T](x: T) -> T:
    static_assert(is_equivalent_to(NewStyleClassScoped[T], LegacyClassScoped[T]))
    static_assert(is_subtype_of(NominalGeneric[T], NewStyleClassScoped[T]))
    static_assert(is_subtype_of(NominalGeneric[T], LegacyClassScoped[T]))
    return x

class NominalConcrete:
    def method(self, input: int) -> None: ...

static_assert(is_assignable_to(NominalConcrete, NewStyleClassScoped))  # error: [missing-type-argument]
static_assert(is_assignable_to(NominalConcrete, LegacyClassScoped))  # error: [missing-type-argument]
static_assert(is_assignable_to(NominalGeneric[int], NewStyleClassScoped))  # error: [missing-type-argument]
static_assert(is_assignable_to(NominalGeneric[int], LegacyClassScoped))  # error: [missing-type-argument]
static_assert(is_assignable_to(NominalGeneric, NewStyleClassScoped[int]))  # error: [missing-type-argument]
static_assert(is_assignable_to(NominalGeneric, LegacyClassScoped[int]))  # error: [missing-type-argument]

# `NewStyleClassScoped` is implicitly `NewStyleClassScoped[Unknown]`,
# and there exist fully static materializations of `NewStyleClassScoped[Unknown]`
# where `Nominal` would not be a subtype of the given materialization,
# hence there is no subtyping relation:
static_assert(not is_subtype_of(NominalConcrete, NewStyleClassScoped))  # error: [missing-type-argument]
static_assert(not is_subtype_of(NominalConcrete, LegacyClassScoped))  # error: [missing-type-argument]

# Similarly, `NominalGeneric` is implicitly `NominalGeneric[Unknown`]
static_assert(not is_subtype_of(NominalGeneric, NewStyleClassScoped[int]))  # error: [missing-type-argument]
static_assert(not is_subtype_of(NominalGeneric, LegacyClassScoped[int]))  # error: [missing-type-argument]

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

FunctionT = TypeVar("FunctionT")

class LegacyFunctionScoped(Protocol):
    def f(self, input: FunctionT) -> FunctionT: ...

class UsesSelf(Protocol):
    def g(self: Self) -> Self: ...

class NominalNewStyle:
    def f[T](self, input: T) -> T:
        return input

class NominalLegacy:
    def f(self, input: FunctionT) -> FunctionT:
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

class ConcreteMethod(Protocol):
    def f(self, input: int) -> int: ...

class GenericReceiver:
    def f[T](self: T, input: T) -> T:
        return self

class GradualReceiverProtocol(Protocol):
    def method(self: list[Any]) -> None: ...

class GradualReceiverImplementation(list[int]):
    def method(self: list[Any]) -> None: ...

class ExplicitReceiverProtocol(Protocol):
    def method(self: "ExplicitReceiverProtocol") -> None: ...

class StructuralExplicitReceiver:
    def method(self: ExplicitReceiverProtocol) -> None: ...

class OverloadedExplicitReceiverProtocol(Protocol):
    def overloaded(self: str, value: int | str) -> int: ...

class OverloadedExplicitReceiverImplementation:
    @overload
    def overloaded(self, value: int) -> int: ...
    @overload
    def overloaded(self, value: str) -> int: ...
    def overloaded(self, value: int | str) -> int:
        return 1

class ReceiverOnly(Protocol):
    def method(self) -> None: ...

class InvalidBoundedReceiver:
    # TODO: Use `BoundTypeVarInstance::valid_specializations` to reject this receiver.
    def method[T: int](self: T) -> None: ...

class InvalidConstrainedReceiver:
    # TODO: Use `BoundTypeVarInstance::valid_specializations` to reject this receiver.
    def method[T: (int, str)](self: T) -> None: ...

static_assert(is_equivalent_to(LegacyFunctionScoped, NewStyleFunctionScoped))
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

# Binding `GenericReceiver.f` adds the constraint `GenericReceiver <= T`. It cannot choose
# `T = int`, so the resulting bound method does not satisfy `ConcreteMethod.f`.
static_assert(not is_assignable_to(GenericReceiver, ConcreteMethod))
static_assert(not is_subtype_of(GenericReceiver, ConcreteMethod))

# Specializing the receiver constraint to `GradualReceiverImplementation` must preserve the
# assignability relation that produced it; `list[int]` is assignable to, but not a subtype of,
# `list[Any]`.
static_assert(is_assignable_to(GradualReceiverImplementation, GradualReceiverProtocol))

# Checking the receiver constraint requires the same protocol relation that is already in
# progress. The recursive check should terminate and establish the structural relation.
static_assert(is_assignable_to(StructuralExplicitReceiver, ExplicitReceiverProtocol))
static_assert(is_subtype_of(StructuralExplicitReceiver, ExplicitReceiverProtocol))

# Aggregating the implementation's overloads covers the visible `int | str` parameter, but the
# implementation's concrete receiver does not satisfy the protocol's explicit `str` receiver.
static_assert(not is_assignable_to(OverloadedExplicitReceiverImplementation, OverloadedExplicitReceiverProtocol))
static_assert(not is_subtype_of(OverloadedExplicitReceiverImplementation, OverloadedExplicitReceiverProtocol))

static_assert(is_assignable_to(InvalidBoundedReceiver, ReceiverOnly))
static_assert(is_subtype_of(InvalidBoundedReceiver, ReceiverOnly))
static_assert(is_assignable_to(InvalidConstrainedReceiver, ReceiverOnly))
static_assert(is_subtype_of(InvalidConstrainedReceiver, ReceiverOnly))

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

## Module objects with static-method protocol members

Module objects implement protocols through their public interface. A module-level function can
therefore satisfy an ordinary or static method member with the same signature.

`factory.py`:

```py
size: int = 1

def make(value: int) -> str:
    return str(value)
```

`main.py`:

```py
from typing import Protocol

import factory

class FactoryObject(Protocol):
    size: int
    def make(self, value: int) -> str: ...

class FactoryModule(Protocol):
    size: int

    @staticmethod
    def make(value: int) -> str: ...

factory_object: FactoryObject = factory
factory_module: FactoryModule = factory
```

## Class objects with class-method protocol members

A class object implements a protocol when its directly accessible members have compatible types. The
corresponding member does not also need to exist on the class object's metaclass:

```py
from typing import Protocol

class Parser(Protocol):
    @classmethod
    def parse(cls, value: str) -> int: ...

class IntParser:
    @classmethod
    def parse(cls, value: str) -> int:
        return int(value)

parser: Parser = IntParser
```

## Class objects and `Self`-returning class-method protocol members

When a class object is checked against a class-method protocol member, `Self` in the protocol
signature names the class object. A class method that returns `Self` returns an instance and cannot
satisfy that requirement; a class method that returns `type[Self]` can satisfy a `type[C]`
candidate:

```py
from typing import Protocol, TypeVar
from typing_extensions import Self
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_assignable_to

class FactoryProtocol(Protocol):
    @classmethod
    def make(cls) -> Self: ...

class ExplicitReceiverFactoryProtocol(Protocol):
    @classmethod
    def make(cls: type[Self]) -> Self: ...

class Factory:
    @classmethod
    def make(cls) -> Self:
        return cls()

class ExplicitReceiverFactory:
    @classmethod
    def make(cls: type[Self]) -> Self:
        return cls()

class BadFactory:
    @classmethod
    def make(cls) -> int:
        return 1

class ClassObjectFactory:
    @classmethod
    def make(cls) -> type[Self]:
        return cls

static_assert(not is_assignable_to(TypeOf[Factory], FactoryProtocol))
static_assert(not is_assignable_to(TypeOf[ExplicitReceiverFactory], ExplicitReceiverFactoryProtocol))
static_assert(not is_assignable_to(TypeOf[BadFactory], FactoryProtocol))
static_assert(is_assignable_to(type[ClassObjectFactory], FactoryProtocol))

T = TypeVar("T", bound=FactoryProtocol)

def exact_factory(value: T) -> T:
    return value.make()

exact_factory(Factory)  # error: [invalid-argument-type]
exact_factory(ClassObjectFactory)  # error: [invalid-argument-type]

def _(factory: type[ClassObjectFactory]) -> None:
    exact_factory(factory)
```

## Class objects and `Self`-returning instance-method protocol members

A class object can satisfy a protocol with a regular instance-method member if the class object's
directly accessible member has a compatible bound signature. Class and static methods therefore
work, but a regular instance method does not: accessing it through the class produces an unbound
function rather than a method bound to the class object. If the protocol method returns `Self`, the
implementation must return the class object, not an instance of the class.

```py
from typing import Protocol, TypeVar
from typing_extensions import Self
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_assignable_to

class CopierProtocol(Protocol):
    def copy(self) -> Self: ...

class PlainCopierProtocol(Protocol):
    def copy(self) -> str: ...

class Copier:
    def copy(self) -> Self:
        return self

class ClassCopier:
    @classmethod
    def copy(cls) -> Self:
        return cls()

class StaticCopier:
    @staticmethod
    def copy() -> "StaticCopier":
        return StaticCopier()

class ClassObjectCopier:
    @classmethod
    def copy(cls) -> type[Self]:
        return cls

class PlainClassCopier:
    @classmethod
    def copy(cls) -> str:
        return "copy"

class PlainStaticCopier:
    @staticmethod
    def copy() -> str:
        return "copy"

class CopierMeta(type):
    def copy(cls) -> "BadDirectCopier":
        return BadDirectCopier()

class BadDirectCopier(metaclass=CopierMeta):
    def copy(self, value: int) -> Self:
        return self

static_assert(is_assignable_to(Copier, CopierProtocol))
static_assert(not is_assignable_to(TypeOf[Copier], CopierProtocol))
static_assert(not is_assignable_to(TypeOf[ClassCopier], CopierProtocol))
static_assert(not is_assignable_to(TypeOf[StaticCopier], CopierProtocol))
static_assert(is_assignable_to(type[ClassObjectCopier], CopierProtocol))
static_assert(is_assignable_to(TypeOf[PlainClassCopier], PlainCopierProtocol))
static_assert(is_assignable_to(TypeOf[PlainStaticCopier], PlainCopierProtocol))
# The metaclass method is compatible, but ordinary protocol methods describe direct access on the
# class object, where `BadDirectCopier.copy` is an incompatible unbound function.
static_assert(not is_assignable_to(TypeOf[BadDirectCopier], CopierProtocol))

T = TypeVar("T", bound=CopierProtocol)

def exact_copy(value: T) -> T:
    return value.copy()

exact_copy(ClassCopier)  # error: [invalid-argument-type]
exact_copy(StaticCopier)  # error: [invalid-argument-type]
exact_copy(ClassObjectCopier)  # error: [invalid-argument-type]

def _(copier: type[ClassObjectCopier]) -> None:
    exact_copy(copier)
```

## Class objects and dunder instance-method protocol members

Special methods are looked up on the meta-type of an object when testing assignability and
subtyping, matching Python's special-method lookup. We therefore understand that `IterableClass`
here is a subtype of `Iterable[int]` even though `IterableClass.__iter__` has the wrong signature:

```py
from typing import Iterable, Iterator
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of

class Meta(type):
    def __iter__(self) -> Iterator[int]:
        yield from range(42)

class IterableClass(metaclass=Meta):
    def __iter__(self) -> Iterator[str]:
        yield from "abc"

static_assert(is_subtype_of(TypeOf[IterableClass], Iterable[int]))

class DirectIterable:
    @classmethod
    def __iter__(cls) -> Iterator[int]:
        yield from range(42)

iterable: Iterable[int] = DirectIterable  # snapshot
```

```snapshot
error[invalid-assignment]: Object of type `<class 'DirectIterable'>` is not assignable to `Iterable[int]`
  --> src/mdtest_snippet.py:20:11
   |
20 | iterable: Iterable[int] = DirectIterable  # snapshot
   |           -------------   ^^^^^^^^^^^^^^ Incompatible value of type `<class 'DirectIterable'>`
   |           |
   |           Declared type
   |
info: type `<class 'DirectIterable'>` is not assignable to protocol `Iterable[int]`
info: └── protocol member `__iter__` is not defined on type `<class 'DirectIterable'>`
info:     └── special methods must be defined on the meta-type when matching a protocol
```

A custom dunder such as `__custom__` is an ordinary method: it is accessed directly on the class
object and does not use Python's special-method lookup.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_assignable_to

class CustomProtocol(Protocol):
    def __custom__(self, value: int) -> str: ...

class Custom:
    @classmethod
    def __custom__(cls, value: int) -> str:
        return str(value)

static_assert(is_assignable_to(TypeOf[Custom], CustomProtocol))
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
from collections.abc import Callable
from typing import Protocol, overload
from typing_extensions import Self
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to, is_equivalent_to, is_disjoint_from

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

class NMaybeCallable:
    x: Callable[[int], str] | None

class F:
    def __call__(self, val: int) -> str:
        return "foo"

class NObject:
    x: object = F()

class NStaticMethodShadowed(NStaticMethodGood):
    def __init__(self) -> None:
        self.x: int = 1

class PFactory(Protocol):
    @classmethod
    def create(cls) -> Self: ...

class Factory:
    @classmethod
    def create(cls) -> Self:
        return cls()

class BadFactory:
    @classmethod
    def create(cls) -> int:
        return 42

class POverloadedFactory(Protocol):
    @overload
    @classmethod
    def create(cls, value: int) -> Self: ...
    @overload
    @classmethod
    def create(cls, value: str) -> Self: ...

class OverloadedFactory:
    @overload
    @classmethod
    def create(cls, value: int) -> Self: ...
    @overload
    @classmethod
    def create(cls, value: str) -> Self: ...
    @classmethod
    def create(cls, value: int | str) -> Self:
        return cls()

# `PClassMethod.x` and `PStaticMethod.x` evaluate to callable types with equivalent signatures
# whether you access them on the protocol class or instances of the protocol.
# That means that they are equivalent protocols!
static_assert(is_equivalent_to(PClassMethod, PStaticMethod))

static_assert(not is_assignable_to(NNotCallable, PClassMethod))
static_assert(not is_assignable_to(NNotCallable, PStaticMethod))
static_assert(not is_disjoint_from(NNotCallable, PClassMethod))
static_assert(not is_disjoint_from(NNotCallable, PStaticMethod))
static_assert(not is_disjoint_from(NMaybeCallable, PStaticMethod))
static_assert(not is_disjoint_from(NObject, PStaticMethod))

# `NInstanceMethod.x` has the correct type when accessed on an instance of
# `NInstanceMethod`, but not when accessed on the class object itself
#
static_assert(not is_assignable_to(NInstanceMethod, PClassMethod))
static_assert(not is_assignable_to(NInstanceMethod, PStaticMethod))

# A nominal type with a `@staticmethod` can satisfy a protocol with a `@classmethod`
# if the staticmethod duck-types the same as the classmethod member
# both when accessed on the class and when accessed on an instance of the class
# The same also applies for a nominal type with a `@classmethod` and a protocol
# with a `@staticmethod` member
static_assert(is_assignable_to(NClassMethodGood, PClassMethod))
static_assert(is_assignable_to(NClassMethodGood, PStaticMethod))
static_assert(is_subtype_of(NClassMethodGood, PClassMethod))
static_assert(is_subtype_of(NClassMethodGood, PStaticMethod))
static_assert(not is_assignable_to(NClassMethodBad, PClassMethod))
static_assert(not is_assignable_to(NClassMethodBad, PStaticMethod))
static_assert(not is_assignable_to(NClassMethodGood | NClassMethodBad, PClassMethod))

static_assert(is_assignable_to(NStaticMethodGood, PClassMethod))
static_assert(is_assignable_to(NStaticMethodGood, PStaticMethod))
static_assert(is_subtype_of(NStaticMethodGood, PClassMethod))
static_assert(is_subtype_of(NStaticMethodGood, PStaticMethod))
static_assert(not is_assignable_to(NStaticMethodBad, PClassMethod))
static_assert(not is_assignable_to(NStaticMethodBad, PStaticMethod))
static_assert(not is_assignable_to(NStaticMethodGood | NStaticMethodBad, PStaticMethod))

# An instance attribute can override an inherited static method.
static_assert(not is_subtype_of(NStaticMethodShadowed, PStaticMethod))

# `Self` in the classmethod signature is bound to the implementation type.
static_assert(is_subtype_of(Factory, PFactory))
static_assert(not is_assignable_to(BadFactory, PFactory))

# Each overload keeps its own `Self` binding.
static_assert(is_subtype_of(OverloadedFactory, POverloadedFactory))
```

A classmethod protocol member does not require a mutable instance attribute. In particular, a frozen
dataclass can satisfy a protocol bound through a classmethod:

```py
from dataclasses import dataclass
from typing import Protocol, TypeVar
from typing_extensions import Self

class Factory(Protocol):
    @classmethod
    def make(cls, value: int) -> Self: ...

T = TypeVar("T", bound=Factory)

def load(target: type[T]) -> None: ...

@dataclass(frozen=True)
class Frozen:
    @classmethod
    def make(cls, value: int) -> Self:
        return cls()

load(Frozen)
```

## Subtyping of protocols with decorated method members

Protocol methods can be decorated with other decorators like `@contextmanager`. When matching
protocol methods to implementations, decorators should be applied consistently:

```py
from typing import Protocol
from collections.abc import Generator
from contextlib import contextmanager
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

class ContextManagerProtocol(Protocol):
    @contextmanager
    def method(self, y: bool = False) -> Generator[None, None, None]: ...

class CorrectImpl:
    @contextmanager
    def method(self, y: bool = False) -> Generator[None, None, None]:
        yield

class AlsoCorrect:
    @contextmanager
    def method(self, y: bool = True) -> Generator[None, None, None]:
        yield

class MissingDecorator:
    def method(self, y: bool = False) -> Generator[None, None, None]:
        yield

static_assert(is_assignable_to(CorrectImpl, ContextManagerProtocol))
static_assert(is_assignable_to(AlsoCorrect, ContextManagerProtocol))
static_assert(not is_assignable_to(MissingDecorator, ContextManagerProtocol))
```

A decorator with a precise callable return type preserves the signatures of class and static
protocol methods:

```py
from collections.abc import Callable
from typing import ParamSpec, Protocol, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of

P = ParamSpec("P")
R = TypeVar("R")

def preserve_signature(function: Callable[P, R]) -> Callable[P, R]:
    return function

class StaticProtocol(Protocol):
    @staticmethod
    @preserve_signature
    def method(value: int) -> str: ...

class StaticImplementation:
    @staticmethod
    def method(value: int) -> str:
        return str(value)

class ClassProtocol(Protocol):
    @classmethod
    @preserve_signature
    def method(cls, value: int) -> str: ...

class ClassImplementation:
    @classmethod
    def method(cls, value: int) -> str:
        return str(value)

static_assert(is_subtype_of(StaticImplementation, StaticProtocol))
static_assert(is_subtype_of(ClassImplementation, ClassProtocol))
```

## Equivalence of protocols with method or property members

Two protocols `P1` and `P2`, both with a method member `x`, are considered equivalent if the
signature of `P1.x` is equivalent to the signature of `P2.x`, even though ty would normally model
any two function definitions as inhabiting distinct function-literal types. The same is also true
for property members.

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to

class P1(Protocol):
    def x(self, y: int) -> None: ...
    @property
    def y(self) -> str: ...
    @property
    def z(self) -> bytes: ...
    @z.setter
    def z(self, value: int) -> None: ...

class P2(Protocol):
    def x(self, y: int) -> None: ...
    @property
    def y(self) -> str: ...
    @property
    def z(self) -> bytes: ...
    @z.setter
    def z(self, value: int) -> None: ...

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
static_assert(is_equivalent_to(P3, P4))
```

As with protocols that only have non-method members, this also holds true when they appear in
differently ordered unions:

```py
class A: ...
class B: ...

static_assert(is_equivalent_to(A | B | P1, P2 | B | A))
static_assert(is_equivalent_to(A | B | P3, P4 | B | A))
```

## Subtyping between two protocol types with method members

A protocol `PSub` with a method member can be considered a subtype of a protocol `PSuper` with a
method member if the signature of the member on `PSub` is a subtype of the signature of the member
on `PSuper`:

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

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
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

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

static_assert(not is_assignable_to(Method, PropertyNotReturningCallable))
static_assert(not is_assignable_to(Method, PropertyWithIncorrectSignature))
```

However, a protocol with a method member can never be considered a subtype of a protocol with a
writable property member of the same name, as method members are covariant and immutable:

```py
class ReadWriteProperty(Protocol):
    @property
    def f(self) -> Callable[[], bool]: ...
    @f.setter
    def f(self, val: Callable[[], bool]): ...

static_assert(not is_assignable_to(Method, ReadWriteProperty))
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

The `ClassVar[int]` example above demonstrates that a `ClassVar` member is readable through both the
instance and the class. That availability alone does not make a callable `ClassVar` a method. Both
reads of a `ClassVar[Callable[[], bool]]` have the same callable type, whereas a method has a bound
instance type and a distinct unbound class type:

```py
from typing import ClassVar

class ClassVarAttribute(Protocol):
    f: ClassVar[Callable[[], bool]]

static_assert(not is_subtype_of(ClassVarAttribute, Method))
static_assert(not is_assignable_to(ClassVarAttribute, Method))

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
    if isinstance(arg, HasX):  # error: [isinstance-against-protocol]
        reveal_type(arg)  # revealed: HasX
    else:
        reveal_type(arg)  # revealed: ~HasX

    if issubclass(arg2, HasX):  # error: [isinstance-against-protocol]
        reveal_type(arg2)  # revealed: type[HasX]
    else:
        reveal_type(arg2)  # revealed: type & ~type[HasX]
```

A protocol class decorated with `@typing(_extensions).runtime_checkable` *can* be used as the second
argument to `isinstance()` at runtime:

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

@runtime_checkable
class OnlyClassmethodMembers(Protocol):
    @classmethod
    def method(cls) -> None: ...

@runtime_checkable
class MultipleNonMethodMembers(Protocol):
    b: int
    a: int

def f(arg1: type):
    # error: [isinstance-against-protocol] "`RuntimeCheckableHasX` cannot be used as the second argument to `issubclass` as it is a protocol with non-method members"
    if issubclass(arg1, RuntimeCheckableHasX):
        reveal_type(arg1)  # revealed: type[RuntimeCheckableHasX]
    else:
        reveal_type(arg1)  # revealed: type & ~type[RuntimeCheckableHasX]

    if issubclass(arg1, MultipleNonMethodMembers):  # error: [isinstance-against-protocol]
        reveal_type(arg1)  # revealed: type[MultipleNonMethodMembers]
    else:
        reveal_type(arg1)  # revealed: type & ~type[MultipleNonMethodMembers]

    if issubclass(arg1, OnlyMethodMembers):  # no error!
        reveal_type(arg1)  # revealed: type[OnlyMethodMembers]
    else:
        reveal_type(arg1)  # revealed: type & ~type[OnlyMethodMembers]

    if issubclass(arg1, OnlyClassmethodMembers):  # no error!
        reveal_type(arg1)  # revealed: type[OnlyClassmethodMembers]
    else:
        reveal_type(arg1)  # revealed: type & ~type[OnlyClassmethodMembers]
```

The same diagnostics are also emitted when protocol classes appear inside a tuple passed as the
second argument to `isinstance()` or `issubclass()`:

```py
def g(arg: object, arg2: type):
    isinstance(arg, (HasX, RuntimeCheckableHasX))  # error: [isinstance-against-protocol]
    isinstance(arg, (HasX, int))  # error: [isinstance-against-protocol]

    # error: [isinstance-against-protocol]
    # error: [isinstance-against-protocol]
    issubclass(arg2, (HasX, RuntimeCheckableHasX))

    issubclass(arg2, (HasX, OnlyMethodMembers))  # error: [isinstance-against-protocol]
```

This includes nested tuples:

```py
def g2(arg: object, arg2: type):
    isinstance(arg, (int, (HasX, str)))  # error: [isinstance-against-protocol]

    # error: [isinstance-against-protocol]
    # error: [isinstance-against-protocol]
    issubclass(arg2, (int, (HasX, RuntimeCheckableHasX)))
```

This also works when the tuple is not a literal in the source:

```py
classes = (HasX, int)

def h(arg: object):
    isinstance(arg, classes)  # error: [isinstance-against-protocol]
```

## Match class patterns and protocols

<!-- snapshot-diagnostics -->

Similar to `isinstance()`, using a non-runtime-checkable protocol class in a match class pattern
will raise `TypeError` at runtime. We emit an error for these cases:

```py
from typing_extensions import Protocol, runtime_checkable

class HasX(Protocol):
    x: int

@runtime_checkable
class RuntimeCheckableHasX(Protocol):
    x: int

def match_non_runtime_checkable(arg: object):
    match arg:
        case HasX():  # error: [isinstance-against-protocol]
            reveal_type(arg)  # revealed: HasX
        case _:
            reveal_type(arg)  # revealed: ~HasX

def match_runtime_checkable(arg: object):
    match arg:
        case RuntimeCheckableHasX():  # no error!
            reveal_type(arg)  # revealed: RuntimeCheckableHasX
        case _:
            reveal_type(arg)  # revealed: ~RuntimeCheckableHasX
```

The same applies to nested class patterns:

```py
class Wrapper:
    inner: object

def match_nested_non_runtime_checkable(arg: Wrapper):
    match arg:
        case Wrapper(inner=HasX()):  # error: [isinstance-against-protocol]
            pass
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
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_assignable_to

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
from ty_extensions._internal import TypeOf

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

Type-variable inference also uses static and class `__call__` members:

```py
from typing import Protocol, TypeVar

CallbackT = TypeVar("CallbackT")

class StaticCallback(Protocol[CallbackT]):
    @staticmethod
    def __call__(value: CallbackT) -> CallbackT: ...

class ClassCallback(Protocol[CallbackT]):
    @classmethod
    def __call__(cls, value: CallbackT) -> CallbackT: ...

def use_static(callback: StaticCallback[CallbackT]) -> CallbackT:
    raise NotImplementedError

def use_class(callback: ClassCallback[CallbackT]) -> CallbackT:
    raise NotImplementedError

def identity(value: int) -> int:
    return value

reveal_type(use_static(identity))  # revealed: int
reveal_type(use_class(identity))  # revealed: int
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

Specializing a type variable to `Any` does not make variadic parameters gradual. The gradual form
requires the parameters to be explicitly or implicitly annotated with `Any` in the function
definition:

```py
from typing import Any, Protocol, TypeVar

T_contra = TypeVar("T_contra", contravariant=True)

class Variadic(Protocol[T_contra]):
    def __call__(self, *args: T_contra, **kwargs: T_contra) -> None: ...

class NoArgs(Protocol):
    def __call__(self) -> None: ...

def _(source: NoArgs):
    target: Variadic[Any] = source  # error: [invalid-assignment]
```

## Class constructors and static callback protocols

A class object's call signature comes from its constructor. An unrelated `__call__` method on the
class's instances does not replace that constructor signature when matching a static callback
protocol:

```py
from typing import Protocol

class Product:
    def __init__(self, value: int) -> None:
        self.value = value

    def __call__(self, text: str) -> str:
        return text

class Constructor(Protocol):
    @staticmethod
    def __call__(value: int) -> Product: ...

constructor: Constructor = Product
```

## Generic protocols and union arguments

When a union is passed to a parameter annotated as a generic protocol, each union element can
satisfy the protocol with a different specialization. For `IntBox | StrBox` assigned to `Box[T]`,
`IntBox` satisfies `Box[int]` and `StrBox` satisfies `Box[str]`, so `T` is inferred as `int | str`.
Other type variables in the same call are still inferred from their corresponding arguments:

```py
from typing import Protocol, TypeVar

T = TypeVar("T")
U = TypeVar("U")

class Box(Protocol[T]):
    def get(self) -> T: ...

class IntBox:
    def get(self) -> int:
        return 1

class StrBox:
    def get(self) -> str:
        return ""

def infer_protocol_union_box(x: Box[T], y: U) -> tuple[T, U]:
    raise NotImplementedError

def check_protocol_union_box(x: IntBox | StrBox):
    reveal_type(infer_protocol_union_box(x, 1))  # revealed: tuple[int | str, Literal[1]]
```

## Nominal subtyping of protocols

Protocols can participate in nominal subtyping as well as structural subtyping. The main use case
for this is that it allows users an "escape hatch" to force a type checker to consider another type
to be a subtype of a given protocol, even if the other type violates the Liskov Substitution
Principle in some way.

```py
from typing import Protocol, final
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_disjoint_from

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
from ty_extensions._internal import is_singleton, is_single_valued

class WeirdAndWacky(Protocol):
    @property
    def __class__(self) -> Callable[[], None]: ...

reveal_type(is_singleton(WeirdAndWacky))  # revealed: Literal[False]
reveal_type(is_single_valued(WeirdAndWacky))  # revealed: Literal[False]
```

## Integration test: `typing.SupportsIndex` and `typing.Sized`

`typing.SupportsIndex` and `typing.Sized` are two protocols that are very commonly used in the wild.

```py
from typing import Any, SupportsIndex, Sized, Literal

def one(some_int: int, some_literal_int: Literal[1], some_indexable: SupportsIndex):
    a: SupportsIndex = some_int
    b: SupportsIndex = some_literal_int
    c: SupportsIndex = some_indexable

def two(some_list: list[Any], some_tuple: tuple[int, str], some_sized: Sized):
    a: Sized = some_list
    b: Sized = some_tuple
    c: Sized = some_sized
```

## Recursive protocols

### Properties

```py
from __future__ import annotations

from typing import Protocol, Any, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to, is_subtype_of, is_equivalent_to

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
from ty_extensions import Intersection, Not, static_assert
from ty_extensions._internal import is_assignable_to, is_equivalent_to

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
    reveal_type(r.subtype_of)  # revealed: type[Recursive]
    reveal_type(r.generic)  # revealed: GenericC[Recursive]
    reveal_type(r.method(r))  # revealed: Recursive
    reveal_type(r.nested)  # revealed: Recursive | ((Recursive, tuple[Recursive, Recursive], /) -> Recursive)

    reveal_type(r.method(r).callable1(1).direct.t[1][1])  # revealed: Recursive
```

### Mutually-recursive protocols

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to

class Foo(Protocol):
    x: "Bar"

class Bar(Protocol):
    x: Foo

static_assert(is_equivalent_to(Foo, Bar))
```

### Disjointness of recursive protocol and recursive final type

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_disjoint_from

class Proto(Protocol):
    x: "Proto"

class Nominal:
    x: "Nominal"

static_assert(not is_disjoint_from(Proto, Nominal))
```

### Regression test: recursive protocol through `dict.items()`

```py
from __future__ import annotations

from typing import Protocol

class IntArray(Protocol):
    def __add__(self, other: IntArray | int) -> IntArray: ...
    def __getitem__(self, key: slice) -> IntArray: ...

data: dict[str, IntArray] = {}
indexed_data = {k: v[0:10] for k, v in data.items()}

reveal_type(indexed_data)  # revealed: dict[str, IntArray]
```

### Regression test: `dict()` overloads with tuple-of-tuples input

This is a regression test for [ty#3026](https://github.com/astral-sh/ty/issues/3026). Matching the
`dict()` overloads that accept `_typeshed.SupportsKeysAndGetItem` against a tuple of tuples used to
trigger exponential behavior before we rejected the protocol candidates.

```py
output = dict((
    ("0", 0),
    ("1", 1),
    ("2", 2),
    ("3", 3),
    ("4", 4),
    ("5", 5),
    ("6", 6),
    ("7", 7),
    ("8", 8),
    ("9", 9),
    ("10", 10),
    ("11", 11),
    ("12", 12),
    ("13", 13),
    ("14", 14),
    ("15", 15),
    ("16", 16),
    ("17", 17),
    ("18", 18),
    ("19", 19),
    ("20", 20),
    ("21", 21),
    ("22", 22),
    ("23", 23),
))
reveal_type(output)  # revealed: dict[str, int]
```

### Regression test: narrowing with self-referential protocols

This snippet caused us to panic on an early version of the implementation for protocols.

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class A(Protocol):
    def x(self) -> "B | A": ...

@runtime_checkable
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

A redundant cast is reported only if neither type contains `Unknown` nor `Todo`. Inspecting protocol
members for these types must terminate when a protocol refers back to itself.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations
from typing import Any, cast, Protocol

class Iterator[T](Protocol):
    def __iter__(self) -> Iterator[T]: ...

def f(value: Iterator[Any]):
    cast(Iterator[Any], value)  # error: [redundant-cast]
```

### Protocol methods and properties in `cast()`

The `Iterator` example above also ensures that the implicit `self` parameter of an ordinary method
does not make the protocol appear recursive. The method's return type must still be checked.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, cast

from ty_extensions import Unknown

class UnknownMethod[T](Protocol):
    def method(self) -> Unknown: ...

def method(value: UnknownMethod[int]) -> None:
    cast(UnknownMethod[int], value)
```

Property getters follow the same rule: their implicit receiver is ignored, but their return type is
checked.

```py
from typing import Protocol, cast

from ty_extensions import Unknown

class IntProperty[T](Protocol):
    @property
    def value(self) -> int: ...

class UnknownProperty[T](Protocol):
    @property
    def value(self) -> Unknown: ...

def properties(known: IntProperty[int], unknown: UnknownProperty[int]) -> None:
    cast(IntProperty[int], known)  # error: [redundant-cast]
    cast(UnknownProperty[int], unknown)
```

### Specialized protocol type parameters in `cast()`

A type variable's bound does not remain part of a specialized protocol. Here, the `Unknown` bound
has been replaced by `int`, so the cast is redundant.

```py
from typing import Protocol, TypeVar, cast

from ty_extensions import Unknown

T = TypeVar("T", bound=Unknown)

class BoundedProtocol(Protocol[T]):
    value: T

def bounded(value: BoundedProtocol[int]) -> None:
    cast(BoundedProtocol[int], value)  # error: [redundant-cast]
```

### Recursive protocol specializations in `cast()`

A protocol can refer to itself with a different type argument on every step. Since the sequence
`Linked[int]`, `Linked[list[int]]`, and so on never repeats exactly, the inspection stops when it
sees the same protocol definition again. The diagnostic is not reported because a later
specialization could expose `Unknown`.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, cast

class Linked[T](Protocol):
    value: T
    next: "Linked[list[T]]"

def linked(value: Linked[int]) -> None:
    cast(Linked[int], value)
```

An explicit `self` annotation is part of the method's type, so recursion through that annotation
must also terminate.

```py
from typing import Protocol, cast

class ExplicitReceiver[T](Protocol):
    def method(self: "ExplicitReceiver[list[T]]") -> int: ...

def explicit_receiver(value: ExplicitReceiver[int]) -> None:
    cast(ExplicitReceiver[int], value)
```

The diagnostic must be withheld because member lookup can depend on the type argument. In this
example, descriptor overload resolution exposes `Unknown` only through the nested protocol.

```py
from typing import Protocol, cast, overload

from ty_extensions import Unknown

class Descriptor:
    @overload
    def __get__(
        self,
        instance: "DescriptorProtocol[list[int]]",
        owner: type["DescriptorProtocol[list[int]]"],
    ) -> Unknown: ...
    @overload
    def __get__(self, instance: object, owner: type[object]) -> int: ...
    def __get__(self, instance: object, owner: type[object]) -> object:
        return object()

def descriptor(_function: object) -> Descriptor:
    return Descriptor()

class DescriptorProtocol[T](Protocol):
    marker: T
    next: "DescriptorProtocol[list[T]]"

    @descriptor
    def value(self) -> object: ...

def descriptor_specialization(value: DescriptorProtocol[int]) -> None:
    reveal_type((value.value, value.next.value))  # revealed: tuple[int, Unknown]
    cast(DescriptorProtocol[int], value)
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

# error: [unbound-type-variable]
# error: [unbound-type-variable]
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
from typing import Any, Protocol

class Foo[T]: ...

class A(Protocol):
    @property
    def _(self: "A") -> Foo[Any]: ...

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
from ty_extensions import static_assert
from ty_extensions._internal import is_subtype_of, is_equivalent_to, is_disjoint_from

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

Ordinary instance attributes are required only on the object constructed by `N`, so they are not
available through a value of type `type[P]`. Class variables and methods are available because every
inhabitant of `type[P]` must provide them on the class object itself.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, ClassVar, Protocol, Self
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_assignable_to, is_subtype_of

class Foo(Protocol):
    x: int
    y: ClassVar[str]
    def method(self) -> bytes: ...

def _(f: type[Foo]):
    reveal_type(f)  # revealed: type[Foo]
    f.x  # error: [unresolved-attribute]
    f.x = 1  # error: [invalid-assignment]
    reveal_type(f.y)  # revealed: str
    f.y = "foo"  # fine
    f.y = b"bad"  # error: [invalid-assignment]
    reveal_type(f.method)  # revealed: (self, /) -> bytes
    reveal_type(f())  # revealed: Foo
```

Both a particular class object, represented by `TypeOf[C]`, and an arbitrary subclass of `C`,
represented by `type[C]`, are checked structurally. `Bar` fails the protocol requirements, while
`Baz` satisfies them.

```py
class Bar: ...

static_assert(not is_assignable_to(type[Bar], type[Foo]))
static_assert(not is_assignable_to(TypeOf[Bar], type[Foo]))
static_assert(not is_subtype_of(type[Bar], type[Foo]))
static_assert(not is_subtype_of(TypeOf[Bar], type[Foo]))

class Baz:
    y: ClassVar[str] = "foo"
    def __init__(self) -> None:
        self.x = 1
    def method(self) -> bytes:
        return b"foo"

static_assert(is_assignable_to(type[Baz], type[Foo]))
static_assert(is_assignable_to(TypeOf[Baz], type[Foo]))
static_assert(is_subtype_of(type[Baz], type[Foo]))
static_assert(is_subtype_of(TypeOf[Baz], type[Foo]))
```

As stated above, a class object must construct instances that satisfy `Foo` in order to inhabit
`type[Foo]`. The type of instances a class is considered to construct respects the `__call__` of its
metaclass. `Factory` constructs `Baz` instances (and itself has the necessary attributes to satisfy
the classvar/method portion of `Foo`), so it inhabits `type[Foo]`. `BadFactory` constructs `object`,
which does not satisfy `Foo`, so it cannot inhabit `type[Foo]`.

```py
class FactoryMeta(type):
    def __call__(self) -> Baz:
        return Baz()

class Factory(metaclass=FactoryMeta):
    y: ClassVar[str] = "foo"
    def method(self) -> bytes:
        return b"foo"

static_assert(is_assignable_to(TypeOf[Factory], type[Foo]))

class BadFactoryMeta(type):
    def __call__(self) -> object:
        return object()

class BadFactory(Baz, metaclass=BadFactoryMeta): ...

static_assert(not is_assignable_to(TypeOf[BadFactory], type[Foo]))
```

Even when construction returns a `Foo`, the class object itself must provide the required class
variable and method (the instance attribute is not required).

```py
class MissingClassVar(metaclass=FactoryMeta):
    def method(self) -> bytes:
        return b"foo"

static_assert(not is_assignable_to(type[MissingClassVar], type[Foo]))

class MissingMethod(metaclass=FactoryMeta):
    y: ClassVar[str] = "foo"

static_assert(not is_assignable_to(type[MissingMethod], type[Foo]))
```

Conversely, compatible class members are not enough if construction produces an object without the
required instance attribute.

```py
class MissingInstanceAttribute:
    y: ClassVar[str] = "foo"
    def method(self) -> bytes:
        return b"foo"

static_assert(not is_assignable_to(type[MissingInstanceAttribute], type[Foo]))
```

A static method can have the right signature on an instance while lacking the unbound signature
required on the class object.

```py
class StaticMethod:
    y: ClassVar[str] = "foo"
    def __init__(self) -> None:
        self.x = 1
    @staticmethod
    def method() -> bytes:
        return b"foo"

static_assert(not is_assignable_to(type[StaticMethod], type[Foo]))
```

Static methods and class methods declared by a protocol are checked on the candidate class object.
They can be provided by the candidate's metaclass.

```py
class DecoratedMethods(Protocol):
    @staticmethod
    def static(value: int) -> str: ...
    @classmethod
    def class_(cls, value: int) -> str: ...

class DecoratedMethodsImpl:
    @staticmethod
    def static(value: int) -> str:
        return str(value)
    @classmethod
    def class_(cls, value: int) -> str:
        return str(value)

static_assert(is_assignable_to(TypeOf[DecoratedMethodsImpl], type[DecoratedMethods]))

class DecoratedMethodsMeta(type):
    @staticmethod
    def static(value: int) -> str:
        return str(value)
    @classmethod
    def class_(cls, value: int) -> str:
        return str(value)
    def __call__(self) -> DecoratedMethodsImpl:
        return DecoratedMethodsImpl()

class MetaclassOnlyDecoratedMethods(metaclass=DecoratedMethodsMeta): ...

static_assert(is_assignable_to(TypeOf[MetaclassOnlyDecoratedMethods], type[DecoratedMethods]))
```

It is not enough for the constructed instance to acquire callables with matching signatures.

```py
def decorated_method(value: int) -> str:
    return str(value)

class InstanceOnlyDecoratedMethods:
    def __init__(self) -> None:
        self.static = decorated_method
        self.class_ = decorated_method

static_assert(not is_assignable_to(TypeOf[InstanceOnlyDecoratedMethods], type[DecoratedMethods]))

def _(cls: type[DecoratedMethods]) -> None:
    reveal_type(cls.static)  # revealed: (value: int) -> str
    reveal_type(cls.class_)  # revealed: (value: int) -> str
```

`Self` in a class method names the instance constructed by the class object being checked.

```py
class SelfFactory(Protocol):
    @classmethod
    def make(cls) -> Self: ...

class SelfFactoryImpl:
    @classmethod
    def make(cls) -> Self:
        return cls()

static_assert(is_assignable_to(TypeOf[SelfFactoryImpl], type[SelfFactory]))
```

A `@property` declaration requires a readable attribute on the constructed instance, but does not
require the implementation to use `@property` or guarantee that the attribute exists on the class
object.

```py
class PropertyProtocol(Protocol):
    @property
    def value(self) -> int: ...

class PropertyImpl:
    def __init__(self) -> None:
        self.value = 1

static_assert(is_assignable_to(TypeOf[PropertyImpl], type[PropertyProtocol]))

class MissingProperty: ...

static_assert(not is_assignable_to(TypeOf[MissingProperty], type[PropertyProtocol]))

def _(cls: type[PropertyProtocol]) -> None:
    cls.value  # error: [unresolved-attribute]
```

Protocol and abstract class objects are accepted as inhabitants of `type[Foo]`. This is
intentionally more permissive than the typing spec, which requires a concrete class.

```py
from abc import ABC, ABCMeta, abstractmethod

class AbstractFoo(ABC):
    x: int
    y: ClassVar[str] = "foo"
    @abstractmethod
    def method(self) -> bytes: ...

static_assert(is_assignable_to(TypeOf[Foo], type[Foo]))
static_assert(is_assignable_to(TypeOf[AbstractFoo], type[Foo]))
```

A structural implementation can use any subclass of `type` as its metaclass, so `type[Foo]` is not
limited to the protocol class's own metaclass.

```py
static_assert(is_subtype_of(type[Foo], type))
static_assert(not is_subtype_of(type[Foo], ABCMeta))
```

`type[Any]` is assignable to `type[Foo]` but is not a subtype of it.

```py
static_assert(is_assignable_to(type[Any], type[Foo]))
static_assert(not is_subtype_of(type[Any], type[Foo]))
```

An ordinary instance attribute does not satisfy a `ClassVar` requirement on another meta-protocol.

```py
class InstanceAttributeProtocol(Protocol):
    value: int

class ClassVariableProtocol(Protocol):
    value: ClassVar[int]

static_assert(not is_assignable_to(type[InstanceAttributeProtocol], type[ClassVariableProtocol]))
```

A metaclass data descriptor takes precedence over an otherwise compatible unbound instance method.

```py
class HidingMeta(type):
    @property
    def method(cls) -> int:
        return 1

class HiddenMethod(metaclass=HidingMeta):
    y: ClassVar[str] = "foo"
    def __init__(self) -> None:
        self.x = 1
    def method(self) -> bytes:
        return b"foo"

static_assert(not is_assignable_to(TypeOf[HiddenMethod], type[Foo]))
```

## Meta-protocols satisfying instance-method protocols

A value of type `type[P]` is itself a class object, so it can satisfy another protocol through its
directly accessible methods. An ordinary method on `P` remains unbound when accessed through
`type[P]`, while class and static methods are bound. Generic specialization and overloads are
preserved in either case.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Iterable, Iterator, Protocol, Self, overload

class BoundMethod(Protocol):
    def method(self, value: int) -> str: ...

class InstanceMethodSource(Protocol):
    def method(self, value: int) -> str: ...

class UnboundMethod(Protocol):
    def method(self, instance: InstanceMethodSource, /, value: int) -> str: ...

class StaticMethodSource(Protocol):
    @staticmethod
    def method(value: int) -> str: ...

class ClassMethodSource(Protocol):
    @classmethod
    def method(cls, value: int) -> str: ...

def _(
    instance_source: type[InstanceMethodSource],
    static_source: type[StaticMethodSource],
    class_source: type[ClassMethodSource],
) -> None:
    reveal_type(instance_source.method)  # revealed: (self, /, value: int) -> str
    unbound: UnboundMethod = instance_source
    bound: BoundMethod = instance_source  # error: [invalid-assignment]
    reveal_type(static_source.method)  # revealed: (value: int) -> str
    static_bound: BoundMethod = static_source
    reveal_type(class_source.method)  # revealed: (value: int) -> str
    class_bound: BoundMethod = class_source

class GenericBoundMethod[T](Protocol):
    def method(self, value: T) -> T: ...

class GenericStaticMethodSource[T](Protocol):
    @staticmethod
    def method(value: T) -> T: ...

def _(source: type[GenericStaticMethodSource[int]]) -> None:
    reveal_type(source.method)  # revealed: (value: int) -> int
    good: GenericBoundMethod[int] = source
    bad: GenericBoundMethod[str] = source  # error: [invalid-assignment]

class OverloadedBoundMethod(Protocol):
    @overload
    def method(self, value: int) -> int: ...
    @overload
    def method(self, value: str) -> str: ...

class OverloadedStaticMethodSource(Protocol):
    @overload
    @staticmethod
    def method(value: int) -> int: ...
    @overload
    @staticmethod
    def method(value: str) -> str: ...

def _(source: type[OverloadedStaticMethodSource]) -> None:
    reveal_type(source.method)  # revealed: Overload[(value: int) -> int, (value: str) -> str]
    overloaded: OverloadedBoundMethod = source

class Copier(Protocol):
    def copy(self) -> Self: ...

class InstanceFactory(Protocol):
    @classmethod
    def copy(cls) -> Self: ...

class ClassObjectFactory(Protocol):
    @classmethod
    def copy(cls) -> type[Self]: ...

def _(instance_factory: type[InstanceFactory], class_object_factory: type[ClassObjectFactory]) -> None:
    reveal_type(instance_factory.copy)  # revealed: () -> InstanceFactory
    bad: Copier = instance_factory  # error: [invalid-assignment]
    reveal_type(class_object_factory.copy)  # revealed: () -> type[ClassObjectFactory]
    good: Copier = class_object_factory

class IterableSource(Protocol):
    def __iter__(self) -> Iterator[int]: ...

class CustomSource(Protocol):
    @classmethod
    def __custom__(cls, value: int) -> str: ...

class CustomConsumer(Protocol):
    def __custom__(self, value: int) -> str: ...

def _(iterable_source: type[IterableSource], custom_source: type[CustomSource]) -> None:
    reveal_type(iterable_source.__iter__)  # revealed: (self, /) -> Iterator[int]
    iterable: Iterable[int] = iterable_source  # error: [invalid-assignment]
    reveal_type(custom_source.__custom__)  # revealed: (value: int) -> str
    custom: CustomConsumer = custom_source
```

## Generic meta-protocols

Generic protocol arguments are preserved by structural matching, member lookup, and construction.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol
from ty_extensions import static_assert
from ty_extensions._internal import is_assignable_to

class GenericFoo[T](Protocol):
    value: T
    def get(self) -> T: ...

class IntFoo:
    def __init__(self) -> None:
        self.value = 1
    def get(self) -> int:
        return self.value

class GenericFooImpl[T]:
    def __init__(self, value: T) -> None:
        self.value = value
    def get(self) -> T:
        return self.value

static_assert(is_assignable_to(type[IntFoo], type[GenericFoo[int]]))
static_assert(not is_assignable_to(type[IntFoo], type[GenericFoo[str]]))

def _(f: type[GenericFoo[int]]) -> None:
    reveal_type(f.get)  # revealed: (self, /) -> int
    reveal_type(f())  # revealed: GenericFoo[int]
```

Inference derives the protocol argument from exact class objects, generic aliases, and parameters
already annotated as `type[GenericFoo[T]]`.

```py
def infer_meta_protocol[T](cls: type[GenericFoo[T]]) -> T:
    raise NotImplementedError

reveal_type(infer_meta_protocol(IntFoo))  # revealed: int
reveal_type(infer_meta_protocol(GenericFooImpl[int]))  # revealed: int

def _(f: type[GenericFoo[int]]) -> None:
    reveal_type(infer_meta_protocol(f))  # revealed: int
```

For a covariant protocol, inference combines the specializations contributed by each class object in
a union.

```py
class Producer[T](Protocol):
    def get(self) -> T: ...

def infer_producer[T](cls: type[Producer[T]]) -> T:
    raise NotImplementedError

def _(flag: bool) -> None:
    cls = GenericFooImpl[int] if flag else GenericFooImpl[str]
    reveal_type(infer_producer(cls))  # revealed: int | str
```

## Generic substitution of `type[Protocol]`

Passing `type[P]` through a generic identity function preserves its structural meaning, including
inside a union.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, TypeVar

class P(Protocol):
    value: int

class GenericP[T](Protocol):
    value: T

T_identity = TypeVar("T_identity")

def class_identity(cls: type[T_identity]) -> type[T_identity]:
    return cls

def _(cls: type[P]) -> None:
    preserved: type[P] = class_identity(cls)

def _(cls: type[P] | type[int]) -> None:
    preserved: type[P] | type[int] = class_identity(cls)
```

The protocol class object is also accepted by the identity function as an inhabitant of `type[P]`.

```py
reveal_type(class_identity(P))  # revealed: type[P]
```

Generic protocol arguments are also preserved through the identity function.

```py
def _(cls: type[GenericP[int]]) -> None:
    preserved: type[GenericP[int]] = class_identity(cls)
    wrong: type[GenericP[str]] = class_identity(cls)  # error: [invalid-assignment]
```

The same substitution occurs when `type[P]` appears in a generic callable signature such as
`classmethod`.

```py
def predicate(cls: type[P]) -> bool:
    return True

classmethod(predicate)
```

## Meta-types of protocol intersections

Calling `type()` on an intersection retains each positive class constraint.

```py
from typing import Protocol

class RuntimeProtocol(Protocol):
    def get(self) -> object: ...

def _(value: RuntimeProtocol) -> None:
    if isinstance(value, int):
        reveal_type(type(value))  # revealed: type[RuntimeProtocol] & type[int]
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
from ty_extensions._internal import reveal_protocol_interface

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

## The `Generator` protocol's `_ReturnT_co` needs special casing

The `_ReturnT_co` type parameter in the `Generator` protocol is the value of a `yield from` over
that generator, and it's also in the pathway for the return values from `async` functions. (In the
`Awaitable` protocol, `__await__` returns a `Generator`.) So of course if we're asking whether one
type of `Generator` is e.g. assignable to another, and we see that one of them has a `_ReturnT_co`
type of `float` while the other has `str`, we should decide that they're not assignable.

However, zooming in to the implementation details, `_ReturnT_co` is actually the type of the `value`
attribute on the `StopIteration` exception that the `Generator` raises when it's finished. This is
awkward, because protocols don't describe the exceptions that their methods raise. How is ty
supposed to see that incompatible `_ReturnT_co` types imply incompatible `Generator`s?

As of Python 3.13, the `Generator` protocol's `close` method was changed from returning `None` to
returning `_ReturnT_co | None`. This was motivated by an edge case (you tried to cancel a generator,
but it caught the related exception and returned something anyway), but coincidentally it tells ty
what it needs to know: `_ReturnT_co` is something that some method in this protocol returns.
Something with a method that returns `float` isn't assignable to something where the same method
returns `str`.

However, prior to 3.13, the `_ReturnT_co` type only appeared in the `__iter__` method.
Unfortunately, the `__iter__` method on a `Generator` just returns `self`; its return type is the
same `Generator`. That isn't helpful for the assignability question, because all we can say by
looking at `__iter__` is that "`Generator` `A` is assignable to `Generator` `B` if...`Generator` `A`
is assignable to `Generator` `B`." In practice we break this recursive cycle by inserting `Any`, and
we end up ignoring `_ReturnT_co` entirely and saying that things are assignable when they shouldn't
be. But how we break the cycle isn't really the problem; the problem is that the `Generator`
protocol (prior to 3.13) genuinely tells us nothing about how `_ReturnT_co` interacts with
assignability.

As a special case workaround for this, we compare `Generator` implementations *nominally* in
`has_relation_to`. Prior to Python 3.13, this is necessary because `_ReturnT_co` is not structurally
visible. As of Python 3.13, it is necessary because structurally inferring through
`close() -> _ReturnT_co | None` can spuriously infer `None`. The latter workaround can be removed
once [ty#3596](https://github.com/astral-sh/ty/issues/3596) is fixed.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to, is_subtype_of, is_assignable_to
from typing import Generator, Awaitable, Protocol, TypeVar, Any, Protocol

T_co = TypeVar("T_co", covariant=True)

class A: ...
class B: ...

static_assert(not is_equivalent_to(Generator[None, None, A], Generator[None, None, B]))
static_assert(not is_subtype_of(Generator[None, None, A], Generator[None, None, B]))
static_assert(not is_subtype_of(Generator[None, None, B], Generator[None, None, A]))

static_assert(is_equivalent_to(Generator[None, None, A], Generator[None, None, A]))
static_assert(is_subtype_of(Generator[None, None, A], Generator[None, None, A]))
static_assert(is_subtype_of(Generator[None, None, A], Generator[None, None, A]))

# Awaitable is also impacted, since `Awaitable.__await__` returns `Generator`

static_assert(not is_equivalent_to(Awaitable[A], Awaitable[B]))
static_assert(not is_equivalent_to(Awaitable[A], Awaitable[Any]))
static_assert(not is_subtype_of(Awaitable[A], Awaitable[B]))
static_assert(not is_assignable_to(Awaitable[A], Awaitable[B]))

class CustomCovariantProtocol(Protocol[T_co]):
    def foo(self) -> tuple[list[Generator[None, None, T_co]]]: ...

static_assert(not is_equivalent_to(CustomCovariantProtocol[A], CustomCovariantProtocol[B]))
static_assert(not is_equivalent_to(CustomCovariantProtocol[A], CustomCovariantProtocol[Any]))
static_assert(not is_subtype_of(CustomCovariantProtocol[A], CustomCovariantProtocol[B]))
static_assert(not is_assignable_to(CustomCovariantProtocol[A], CustomCovariantProtocol[B]))
```

## The `Generator` protocol's `_ReturnT_co` appears in `close` as of Python 3.13

The same test cases as above, but for Python 3.13 instead of 3.12. In this version `_ReturnT_co`
appears in `Generator`'s `close` method.

```toml
[environment]
python-version = "3.13"
```

```py
from ty_extensions import static_assert
from ty_extensions._internal import is_equivalent_to, is_subtype_of, is_assignable_to
from typing import Generator, Awaitable, TypeVar, Protocol, Any

T_co = TypeVar("T_co", covariant=True)

class A: ...
class B: ...

static_assert(not is_equivalent_to(Generator[None, None, A], Generator[None, None, B]))
static_assert(not is_subtype_of(Generator[None, None, A], Generator[None, None, B]))
static_assert(not is_subtype_of(Generator[None, None, B], Generator[None, None, A]))

static_assert(is_equivalent_to(Generator[None, None, A], Generator[None, None, A]))
static_assert(is_subtype_of(Generator[None, None, A], Generator[None, None, A]))
static_assert(is_subtype_of(Generator[None, None, A], Generator[None, None, A]))

static_assert(not is_equivalent_to(Awaitable[A], Awaitable[B]))
static_assert(not is_equivalent_to(Awaitable[A], Awaitable[Any]))
static_assert(not is_subtype_of(Awaitable[A], Awaitable[B]))
static_assert(not is_assignable_to(Awaitable[A], Awaitable[B]))

class CustomCovariantProtocol(Protocol[T_co]):
    def foo(self) -> tuple[list[Generator[None, None, T_co]]]: ...

static_assert(not is_equivalent_to(CustomCovariantProtocol[A], CustomCovariantProtocol[B]))
static_assert(not is_equivalent_to(CustomCovariantProtocol[A], CustomCovariantProtocol[Any]))
static_assert(not is_subtype_of(CustomCovariantProtocol[A], CustomCovariantProtocol[B]))
static_assert(not is_assignable_to(CustomCovariantProtocol[A], CustomCovariantProtocol[B]))
```

## Inferring async return contexts on Python 3.13 or newer

Regression test for [ty#3583](https://github.com/astral-sh/ty/issues/3583). When inferring the
generic async call in a return statement, the `Awaitable[int]` context should not infer `None`
through `Generator.close()`.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any, Generic, TypeVar

T = TypeVar("T", bound=tuple[Any, ...])

class Select(Generic[T]):
    pass

def first[T](v: T) -> Select[tuple[T]]:
    raise NotImplementedError

async def second[T](query: Select[tuple[T]]) -> T:
    raise NotImplementedError

async def variant_one() -> int:
    result = await second(first(123))
    return result

async def variant_two() -> int:
    return await second(first(123))
```

## TODO

Add tests for:

- More tests for protocols inside `type[]`. [Spec reference][protocols_inside_type_spec].
- Protocols with instance-method members, including:
    - Protocols with methods that have parameters or the return type unannotated
    - Protocols with methods that have parameters or the return type annotated with `Any`
- Assignability of non-instance types to protocols with instance-method members (e.g. a
    class-literal type can be a subtype of `Sized` if its metaclass has a `__len__` method)
- Protocols with methods or property getters that have annotated `self` parameters.
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
