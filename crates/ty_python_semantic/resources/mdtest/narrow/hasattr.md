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
