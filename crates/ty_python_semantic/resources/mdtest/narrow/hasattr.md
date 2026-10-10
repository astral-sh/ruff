# Narrowing using `hasattr()`

## Basic behavior

The builtin function `hasattr()` can narrow nominal and structural types in its positive branch by
intersecting the type with a synthesized protocol. A negative check excludes types whose classes
provide the attribute. We do not check definite initialization of instance attributes, so their
absence does not rule out an instance of a class that only declares them.

```py
from typing import final
from typing_extensions import LiteralString

class NonFinalClass: ...

def _(obj: NonFinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: NonFinalClass & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: object
    else:
        reveal_type(obj)  # revealed: NonFinalClass

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown

    if hasattr(obj, "not-an-identifier"):
        reveal_type(obj)  # revealed: NonFinalClass
    else:
        reveal_type(obj)  # revealed: NonFinalClass
```

For a final class, we recognize that there is no way that an object of `FinalClass` could ever have
a `spam` attribute, so the type is narrowed to `Never`:

```py
@final
class FinalClass: ...

def _(obj: FinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: Never
        reveal_type(obj.spam)  # revealed: Never
    else:
        reveal_type(obj)  # revealed: FinalClass

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown
```

When the corresponding attribute is already defined on the class, positive `hasattr` narrowing does
not change the type. `<Protocol with members 'spam'>` is a supertype of `WithSpam`, and so
`WithSpam & <Protocol …>` simplifies to `WithSpam`. The negative branch is unreachable, including
for methods:

```py
class WithSpam:
    spam: int = 42

    def method(self) -> None: ...
    @classmethod
    def class_method(cls) -> None: ...
    @staticmethod
    def static_method() -> None: ...

def _(obj: WithSpam):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: WithSpam
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: Never

    if not hasattr(obj, "method"):
        reveal_type(obj)  # revealed: Never
    if not hasattr(obj, "class_method"):
        reveal_type(obj)  # revealed: Never
    if not hasattr(obj, "static_method"):
        reveal_type(obj)  # revealed: Never
```

When a class may or may not have a `spam` attribute, `hasattr` narrowing can provide evidence that
the attribute exists. Here, no `possibly-missing-attribute` error is emitted in the `if` branch:

```py
def returns_bool() -> bool:
    return False

class MaybeWithSpam:
    if returns_bool():
        spam: int = 42

def _(obj: MaybeWithSpam):
    # error: [possibly-missing-attribute]
    reveal_type(obj.spam)  # revealed: int

    if hasattr(obj, "spam"):
        reveal_type(obj)  #  revealed: MaybeWithSpam & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: MaybeWithSpam

        # TODO: Ideally, we would emit `[unresolved-attribute]` and reveal `Unknown` here:
        # error: [possibly-missing-attribute]
        reveal_type(obj.spam)  # revealed: int
```

All attribute available on `object` are still available on these synthesized protocols, but
attributes that are not present on `object` are not available:

```py
def f(x: object):
    if hasattr(x, "__qualname__"):
        reveal_type(x.__repr__)  # revealed: bound method object.__repr__() -> str
        reveal_type(x.__str__)  # revealed: bound method object.__str__() -> str
        reveal_type(x.__dict__)  # revealed: dict[str, Any]

        # error: [unresolved-attribute] "Object of type `<Protocol with members '__qualname__'>` has no attribute `foo`"
        reveal_type(x.foo)  # revealed: Unknown
```

Not every object has an instance dictionary, despite the broad `object.__dict__` annotation in
typeshed. Checking for `__dict__` therefore preserves the corresponding protocol constraint.

```py
def has_dictionary(value: object) -> None:
    if hasattr(value, "__dict__"):
        reveal_type(value)  # revealed: <Protocol with members '__dict__'>
```

A protocol can be implemented by classes with or without instance dictionaries, so checking for
`__dict__` preserves both possibilities.

```py
from typing import Protocol

class HasValue(Protocol):
    value: int

def protocol_dictionary(value: HasValue) -> None:
    if hasattr(value, "__dict__"):
        reveal_type(value)  # revealed: HasValue & <Protocol with members '__dict__'>
    else:
        reveal_type(value)  # revealed: HasValue
```

A final slotted class cannot gain an instance dictionary through a subclass, so the positive branch
is unreachable.

```py
@final
class FinalSlotted:
    __slots__ = ()

def no_dictionary(value: FinalSlotted) -> None:
    if hasattr(value, "__dict__"):
        reveal_type(value)  # revealed: Never
```

## Annotated instance attributes can be absent

An annotation does not initialize an instance attribute.

```py
class Annotated:
    value: int

    def initialize(self) -> None:
        if not hasattr(self, "value"):
            reveal_type(self)  # revealed: Self@initialize
            self.value = 1  # no diagnostic

def check(value: Annotated) -> None:
    reveal_type(hasattr(value, "value"))  # revealed: bool
    if hasattr(value, "value"):
        reveal_type(value.value)  # revealed: int
    else:
        reveal_type(value)  # revealed: Annotated
        value.value = "invalid"  # error: [invalid-assignment]
```

## Instance initialization

Assignments in `__init__` also do not make a negative `hasattr` check unreachable.

```py
class Initialized:
    def __init__(self) -> None:
        self.value = 1

def check(value: Initialized) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: Initialized
```

## Inherited uninitialized slots

A slot provides storage for an instance attribute without initializing it, including in subclasses.
It also hides a default inherited from a base class.

```py
class Default:
    value: int = 1

class Base(Default):
    __slots__ = ("value",)
    value: int

class Slotted(Base): ...

def check(value: Slotted) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: Slotted
```

## Unions of instance and class attributes

A negative `hasattr` check excludes union members whose classes provide the attribute, but preserves
members that only declare an instance attribute.

```py
class InstanceAttribute:
    value: int

class ClassAttribute:
    value: int = 1

def check(value: InstanceAttribute | ClassAttribute) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: InstanceAttribute
```

## Inherited defaults behind annotations

An annotation in a subclass does not hide a value inherited from its base class.

```py
class Base:
    value: int = 1

class Derived(Base):
    value: int

def check(value: Derived) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: Never
```

## Declarations in protocols and stubs

Methods declared in protocols and stubs also establish presence. A negative check for `keys`
excludes the corresponding protocol from a union with an iterable. Annotation-only declarations in
stubs do not establish presence, including `ClassVar` declarations.

`methods.pyi`:

```pyi
from typing import ClassVar, Protocol

class HasKeys(Protocol):
    def keys(self) -> list[str]: ...

class WithMethod:
    value: int
    class_value: ClassVar[int]

    def method(self) -> None: ...
```

`main.py`:

```py
from collections.abc import Iterable
from methods import HasKeys, WithMethod

def check_keys(value: HasKeys | Iterable[str]) -> None:
    if not hasattr(value, "keys"):
        reveal_type(value)  # revealed: Iterable[str]

def check_method(value: WithMethod) -> None:
    if not hasattr(value, "method"):
        reveal_type(value)  # revealed: Never

def check_annotations(value: WithMethod) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: WithMethod
    if not hasattr(value, "class_value"):
        reveal_type(value)  # revealed: WithMethod
```

## Properties and custom descriptors

A property getter or descriptor can raise `AttributeError`, so it does not make the negative branch
unreachable.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type) -> int:
        raise AttributeError

def make_descriptor() -> object:
    return Descriptor()

class WithDescriptors:
    descriptor = Descriptor()
    opaque = make_descriptor()

    @property
    def property(self) -> int:
        raise AttributeError

def check(value: WithDescriptors) -> None:
    if not hasattr(value, "descriptor"):
        reveal_type(value)  # revealed: WithDescriptors
    if not hasattr(value, "property"):
        reveal_type(value)  # revealed: WithDescriptors
    if not hasattr(value, "opaque"):
        reveal_type(value)  # revealed: WithDescriptors
```

## Class attributes initialized in methods

Assigning an attribute through `cls` does not establish that it is present before that method runs.

```py
class Initialized:
    @classmethod
    def initialize(cls) -> None:
        cls.value = 1

def check(value: Initialized) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: Initialized
```

## Dataclass field specifiers

A field specifier can be removed from the class by the dataclass decorator. It does not establish
presence, whether the field is left unset or initialized on instances by a factory.

```py
from dataclasses import dataclass, field

@dataclass
class Uninitialized:
    value: int = field(init=False)

@dataclass
class WithFactory:
    value: list[int] = field(default_factory=list)

def check_uninitialized(value: Uninitialized) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: Uninitialized

def check_factory(value: WithFactory) -> None:
    if not hasattr(value, "value"):
        reveal_type(value)  # revealed: WithFactory
```

## Conjunctions with instance checks

A negative attribute check before an `isinstance` check does not exclude a class that only declares
the attribute. Reversing the checks has the same result.

```py
class Annotated:
    value: int

def check(value: object) -> None:
    if not hasattr(value, "value") and isinstance(value, Annotated):
        reveal_type(value)  # revealed: Annotated
    if isinstance(value, Annotated) and not hasattr(value, "value"):
        reveal_type(value)  # revealed: Annotated
```

A class with a bound default is excluded in either order.

```py
class WithDefault:
    value: int = 1

def check_default(value: object) -> None:
    if not hasattr(value, "value") and isinstance(value, WithDefault):
        reveal_type(value)  # revealed: Never
    if isinstance(value, WithDefault) and not hasattr(value, "value"):
        reveal_type(value)  # revealed: Never
```

## Known module attributes

Unconditional module bindings make negative checks unreachable. Module-level declarations in stubs
also establish presence, even when their type is `Any`.

`implementation.py`:

```py
value = 1

def function() -> None: ...
def condition() -> bool:
    return True

if condition():
    conditional = 1
```

`interface.pyi`:

```pyi
from typing import Any

value: int
dynamic: Any

def function() -> None: ...
```

`main.py`:

```py
import implementation
import interface

if not hasattr(implementation, "value"):
    reveal_type(implementation)  # revealed: Never
if not hasattr(implementation, "function"):
    reveal_type(implementation)  # revealed: Never
if not hasattr(implementation, "conditional"):
    reveal_type(implementation)  # revealed: <module 'implementation'>
if not hasattr(interface, "value"):
    reveal_type(interface)  # revealed: Never
if not hasattr(interface, "dynamic"):
    reveal_type(interface)  # revealed: Never
if not hasattr(interface, "function"):
    reveal_type(interface)  # revealed: Never
```

## Uninitialized module attributes

An annotation in a source module does not initialize the attribute.

`annotated.py`:

```py
value: int
```

`main.py`:

```py
import annotated

if not hasattr(annotated, "value"):
    reveal_type(annotated)  # revealed: <module 'annotated'>
```

## Optional module metadata

Built-in modules can lack `__file__`, and modules that are not packages lack `__path__`.

`ordinary.py`:

```py
pass
```

`main.py`:

```py
import ordinary
import sys

if not hasattr(sys, "__file__"):
    reveal_type(sys)  # revealed: <module 'sys'>
if not hasattr(ordinary, "__path__"):
    reveal_type(ordinary)  # revealed: <module 'ordinary'>
```

## Dynamic module attributes

A module's `__getattr__` can raise `AttributeError`, so it does not establish presence of arbitrary
attributes.

`dynamic.py`:

```py
def __getattr__(name: str) -> int:
    raise AttributeError(name)
```

`main.py`:

```py
import dynamic

if not hasattr(dynamic, "missing"):
    reveal_type(dynamic)  # revealed: <module 'dynamic'>
```

## Known class-object attributes

Class bindings, including inherited methods and values, make negative checks unreachable on class
objects. They can also distinguish alternatives in a union of class types.

```py
class Base:
    value = 1

    def method(self) -> None: ...

class Derived(Base): ...
class Other: ...

if not hasattr(Derived, "value"):
    reveal_type(Derived)  # revealed: Never
if not hasattr(Derived, "method"):
    reveal_type(Derived)  # revealed: Never

def check(cls: type[Derived]) -> None:
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: Never
    if not hasattr(cls, "method"):
        reveal_type(cls)  # revealed: Never

def check_union(cls: type[Derived] | type[Other]) -> None:
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: type[Other]
```

## Instance attributes on class objects

An instance annotation or assignment does not create a binding on the class object. Assignments
through `cls` also do not establish presence before that method runs.

```py
class InstanceAttributes:
    declared: int

    def __init__(self) -> None:
        self.initialized = 1

    @classmethod
    def initialize_class(cls) -> None:
        cls.class_value = 1

if not hasattr(InstanceAttributes, "declared"):
    reveal_type(InstanceAttributes)  # revealed: <class 'InstanceAttributes'>
if not hasattr(InstanceAttributes, "initialized"):
    reveal_type(InstanceAttributes)  # revealed: <class 'InstanceAttributes'>

def check(cls: type[InstanceAttributes]) -> None:
    if not hasattr(cls, "declared"):
        reveal_type(cls)  # revealed: type[InstanceAttributes]
    if not hasattr(cls, "initialized"):
        reveal_type(cls)  # revealed: type[InstanceAttributes]
    if not hasattr(cls, "class_value"):
        reveal_type(cls)  # revealed: type[InstanceAttributes]
```

## Descriptor objects on classes

Properties and slot descriptors are present on their defining classes, even when the corresponding
instance attributes can be absent.

```py
class WithDescriptors:
    __slots__ = ("slot",)
    slot: int

    @property
    def value(self) -> int:
        raise AttributeError

if not hasattr(WithDescriptors, "slot"):
    reveal_type(WithDescriptors)  # revealed: Never
if not hasattr(WithDescriptors, "value"):
    reveal_type(WithDescriptors)  # revealed: Never

def check(cls: type[WithDescriptors]) -> None:
    if not hasattr(cls, "slot"):
        reveal_type(cls)  # revealed: Never
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: Never
```

## Stub property objects on classes

Property declarations in stubs also establish presence on class objects. This lets a negative check
distinguish class types with different property declarations.

`classes.pyi`:

```pyi
class Array:
    @property
    def length(self) -> int: ...

class Scalar: ...
```

`main.py`:

```py
from classes import Array, Scalar

if not hasattr(Array, "length"):
    reveal_type(Array)  # revealed: Never

def check(cls: type[Array] | type[Scalar]) -> None:
    if not hasattr(cls, "length"):
        reveal_type(cls)  # revealed: type[Scalar]
```

## Protocol property implementations

A protocol's property object is present on the protocol class itself. Classes implementing that
protocol can instead use an instance attribute, so the property need not exist on their class
objects. Method contracts still establish presence on implementing classes.

```py
from typing import Protocol

class HasValue(Protocol):
    @property
    def value(self) -> int: ...
    def method(self) -> None: ...

if not hasattr(HasValue, "value"):
    reveal_type(HasValue)  # revealed: Never

def check(cls: type[HasValue]) -> None:
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: type[HasValue]
    if not hasattr(cls, "method"):
        reveal_type(cls)  # revealed: Never
```

## Enum properties

Unlike a built-in `property`, `enum.property` can raise `AttributeError` when accessed on its
defining class.

```toml
[environment]
python-version = "3.11"
```

```py
from enum import Enum, property as enum_property

class Choice(Enum):
    @enum_property
    def label(self) -> str:
        return "item"

if not hasattr(Choice, "label"):
    reveal_type(Choice)  # revealed: <class 'Choice'>

def check(cls: type[Choice]) -> None:
    if not hasattr(cls, "label"):
        reveal_type(cls)  # revealed: type[Choice]
```

## Metaclass attributes

Ordinary metaclass values and methods establish presence on class objects. A metaclass's
`__getattr__` does not establish presence of arbitrary attributes, and a property getter can also
raise `AttributeError`.

```py
class Meta(type):
    bound = 1

    def method(cls) -> None: ...
    def __getattr__(cls, name: str) -> int:
        raise AttributeError(name)

    @property
    def value(cls) -> int:
        raise AttributeError

class Dynamic(metaclass=Meta): ...

if not hasattr(Dynamic, "bound"):
    reveal_type(Dynamic)  # revealed: Never
if not hasattr(Dynamic, "method"):
    reveal_type(Dynamic)  # revealed: Never
if not hasattr(Dynamic, "missing"):
    reveal_type(Dynamic)  # revealed: <class 'Dynamic'>
if not hasattr(Dynamic, "value"):
    reveal_type(Dynamic)  # revealed: <class 'Dynamic'>

def check(cls: type[Dynamic]) -> None:
    if not hasattr(cls, "bound"):
        reveal_type(cls)  # revealed: Never
    if not hasattr(cls, "method"):
        reveal_type(cls)  # revealed: Never
    if not hasattr(cls, "missing"):
        reveal_type(cls)  # revealed: type[Dynamic]
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: type[Dynamic]
```

A metaclass property takes precedence over a value assigned in the class body, so that value does
not establish presence either.

```py
class OwnValue(metaclass=Meta):
    value = 1

if not hasattr(OwnValue, "value"):
    reveal_type(OwnValue)  # revealed: <class 'OwnValue'>

def check_own_value(cls: type[OwnValue]) -> None:
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: type[OwnValue]
```

## Class descriptors shadowing metaclass values

A class descriptor takes precedence over an ordinary metaclass value. Its getter can raise
`AttributeError` even though the metaclass provides a value with the same name.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type) -> int:
        raise AttributeError

class Meta(type):
    value = 1

class WithDescriptor(metaclass=Meta):
    value = Descriptor()

if not hasattr(WithDescriptor, "value"):
    reveal_type(WithDescriptor)  # revealed: <class 'WithDescriptor'>

def check(cls: type[WithDescriptor]) -> None:
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: type[WithDescriptor]
```

## Opaque metaclass descriptors

A metaclass attribute typed as `object` could be a data descriptor whose getter takes precedence
over a class-body value and raises `AttributeError`.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type) -> int:
        raise AttributeError

    def __set__(self, instance: object, value: object) -> None: ...

def make_descriptor() -> object:
    return Descriptor()

class Meta(type):
    value = make_descriptor()

class OwnValue(metaclass=Meta):
    value = 1

if not hasattr(OwnValue, "value"):
    reveal_type(OwnValue)  # revealed: <class 'OwnValue'>

def check(cls: type[OwnValue]) -> None:
    if not hasattr(cls, "value"):
        reveal_type(cls)  # revealed: type[OwnValue]
```
