# Instance slots

Classes can declare instance attributes and restrict their instance layout with `__slots__`.

## Slot names declare instance attributes

A slot is a valid instance attribute even when no method assigns to it. It can be read and assigned
without a type error, even though its type is unknown.

```py
class Slotted:
    __slots__ = ("value",)

reveal_type(Slotted().value)  # revealed: Unknown
Slotted().value = 1
```

## Slots create class descriptors

Accessing a slot on the class returns a `MemberDescriptorType` descriptor. This descriptor is not a
`property` and does not expose property attributes.

```py
class Slotted:
    __slots__ = ("value",)

reveal_type(Slotted.value)  # revealed: MemberDescriptorType

def accepts_property(descriptor: property) -> None: ...

accepts_property(Slotted.value)  # error: [invalid-argument-type]
Slotted.value.fget  # error: [unresolved-attribute]
```

## Slot names appear on classes and instances

The name of a slot is included when looking up the available attributes of its class or an instance.

```py
from ty_extensions import static_assert
from ty_extensions._internal import has_member

class Slotted:
    __slots__ = ("value",)

static_assert(has_member(Slotted, "value"))
static_assert(has_member(Slotted(), "value"))
```

## Slot descriptors can be called directly

A slot descriptor can be assigned to its public `MemberDescriptorType` annotation. Calling its
`__get__` method directly then uses the return type declared in typeshed.

```py
from types import MemberDescriptorType

class Slotted:
    __slots__ = ("value",)

descriptor: MemberDescriptorType = Slotted.value
reveal_type(descriptor.__get__(Slotted(), Slotted))  # revealed: Any
```

## Class dictionaries are separate from instance dictionary slots

An instance dictionary slot must not replace the existing namespace exposed by its class.

```py
class WithDictionary:
    __slots__ = ("value", "__dict__")

reveal_type(WithDictionary.__dict__)  # revealed: dict[str, Any]
```

A subclass continues to expose its own class namespace.

```py
class SlottedChild(WithDictionary):
    __slots__ = ()

reveal_type(SlottedChild.__dict__)  # revealed: dict[str, Any]
```

The same rule applies when a class is accessed through a `type` annotation.

```py
def inspect_class(cls: type[WithDictionary]) -> None:
    reveal_type(cls.__dict__)  # revealed: dict[str, Any]
```

## Slot assignments preserve inferred types

Assignments to slotted attributes continue to determine their inferred types.

```py
class Slotted:
    __slots__ = ("value",)

    def __init__(self, value: int) -> None:
        self.value = value

reveal_type(Slotted(1).value)  # revealed: int
```

## Assignments narrow slotted attributes

Writing to an annotated slot narrows later reads, just as it does for an ordinary instance
attribute.

```py
class Slotted:
    __slots__ = ("value",)

    def __init__(self) -> None:
        self.value: int | None = None

def assign(instance: Slotted) -> int:
    instance.value = 1
    reveal_type(instance.value)  # revealed: Literal[1]
    return instance.value
```

A conditional assignment also removes `None` from later reads.

```py
def initialize(instance: Slotted) -> int:
    if instance.value is None:
        instance.value = 1
    return instance.value
```

Narrowing does not change which values can be assigned to the declared attribute.

```py
def reject(instance: Slotted) -> None:
    instance.value = "wrong"  # error: [invalid-assignment]
```

## Assignments narrow slots annotated in class bodies

An annotation in the class body follows the same narrowing rules as an annotation in an initializer.

```py
class Slotted:
    __slots__ = ("value",)
    value: int | None

def assign(instance: Slotted) -> int:
    instance.value = 1
    return instance.value
```

## Assignments do not narrow arbitrary descriptors

Unlike a slot, an arbitrary data descriptor can transform an assigned value in its setter. Later
reads therefore retain the return type of the descriptor's `__get__` method.

```py
class TransformingDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int | None: ...
    def __set__(self, instance: object, value: int) -> None: ...

class DescriptorOwner:
    __slots__ = ()
    value = TransformingDescriptor()

def inspect_descriptor(owner: DescriptorOwner) -> None:
    owner.value = 1
    reveal_type(owner.value)  # revealed: int | None
```

## Annotated slots enforce their declared types

An annotation on a slot controls both attribute reads and assignments.

```py
class Slotted:
    __slots__ = ("value",)
    value: int

reveal_type(Slotted.value)  # revealed: MemberDescriptorType
reveal_type(Slotted().value)  # revealed: int
Slotted().value = 1
Slotted().value = "wrong"  # error: [invalid-assignment]
```

## Slots declared in stub files

A bare stub annotation describes the value stored in a runtime slot without creating a conflicting
class variable.

```pyi
class BareAnnotation:
    __slots__ = ("value",)
    value: int

reveal_type(BareAnnotation.value)  # revealed: MemberDescriptorType
reveal_type(BareAnnotation().value)  # revealed: int
BareAnnotation().value = 1
BareAnnotation().value = "wrong"  # error: [invalid-assignment]
```

An ellipsis placeholder in a stub has the same meaning and does not conflict with its slot.

```pyi
class EllipsisAnnotation:
    __slots__ = ("value",)
    value: str = ...

reveal_type(EllipsisAnnotation().value)  # revealed: str
EllipsisAnnotation().value = "valid"
EllipsisAnnotation().value = 1  # error: [invalid-assignment]
```

## Standard-library slot declarations

Standard-library stubs use ordinary annotations for writable slotted attributes. For example,
`TarInfo.size` remains a writable `int`.

```py
from tarfile import TarInfo

tar_info = TarInfo("example")
tar_info.size = 1
tar_info.size = "wrong"  # error: [invalid-assignment]
```

## Generic slots use the instance's type arguments

A slot in a generic class uses the type arguments of the instance.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Box(Generic[T]):
    __slots__ = ("value",)
    value: T

    def __init__(self, value: T) -> None:
        self.value = value

reveal_type(Box(1).value)  # revealed: int
Box(1).value = "wrong"  # error: [invalid-assignment]
```

## Slot attributes can be deleted

Slot descriptors support deleting their stored values as well as reading and writing them.

```py
class Slotted:
    __slots__ = ("value",)

instance = Slotted()
instance.value = 1
del instance.value
```

## Slots initialized in `__new__`

Initializing a slot in `__new__` must not interfere with an augmented assignment to the same slot.

```py
class Counter:
    __slots__ = ("value",)

    def __new__(cls):
        instance = super().__new__(cls)
        instance.value = 0
        return instance

    def increment(self) -> None:
        self.value += 1

reveal_type(Counter().value)  # revealed: Unknown
```

## Supported ways to declare slots

A single string declares one slot.

```py
class StringSlots:
    __slots__ = "value"

reveal_type(StringSlots().value)  # revealed: Unknown
```

A tuple can declare multiple slots.

```py
class TupleSlots:
    __slots__ = ("first", "second")

reveal_type(TupleSlots().first)  # revealed: Unknown
reveal_type(TupleSlots().second)  # revealed: Unknown
```

A list can also provide the slot names.

```py
class ListSlots:
    __slots__ = ["value"]

reveal_type(ListSlots().value)  # revealed: Unknown
```

A set can provide the slot names as its elements.

```py
class SetSlots:
    __slots__ = {"value"}

reveal_type(SetSlots().value)  # revealed: Unknown
```

When `__slots__` is a dictionary, its keys are the slot names.

```py
class DictionarySlots:
    __slots__ = {"value": "Documentation for the slot."}

reveal_type(DictionarySlots().value)  # revealed: Unknown
```

## Annotated and indirect slot declarations

An annotation on `__slots__` does not hide its runtime value.

```py
class AnnotatedSlots:
    __slots__: tuple[str, ...] = ("value",)

reveal_type(AnnotatedSlots().value)  # revealed: Unknown
```

A statically known tuple can also be supplied through another variable.

```py
slot_names = ("value",)

class IndirectSlots:
    __slots__ = slot_names

reveal_type(IndirectSlots().value)  # revealed: Unknown
```

## Mutated slot declarations

Slot names are taken from the original literal. Later changes to that literal are not evaluated, so
an appended name is not treated as an available slot.

```py
class MutatedSlots:
    __slots__ = ["value"]
    __slots__.append("extra")

    def __init__(self) -> None:
        self.value = 1
        self.extra = 2  # error: [unresolved-attribute]
```

## Dynamic slot declarations

When the slot names cannot be determined statically, attribute writes remain permissive.

```py
def choose_slots() -> tuple[str, ...]:
    return ("value",)

class DynamicSlots:
    __slots__ = choose_slots()

    def __init__(self) -> None:
        self.value = 1
        self.extra = 2

reveal_type(DynamicSlots().extra)  # revealed: int
```

## Inherited slots

A slotted subclass can use slots declared by any of its base classes.

```py
class Base:
    __slots__ = ("base_value",)

class Child(Base):
    __slots__ = ("child_value",)

    def __init__(self) -> None:
        self.base_value = 1
        self.child_value = 2

reveal_type(Child.base_value)  # revealed: MemberDescriptorType
reveal_type(Child().base_value)  # revealed: int
```

## Slots use annotations inherited from base classes

A class with empty `__slots__` can declare an instance attribute without providing a slot for it.
The annotation alone does not make the attribute writable.

```py
class Base:
    __slots__ = ()
    value: int

Base().value = 1  # error: [unresolved-attribute]
```

A subclass can create the missing slot. Its inherited annotation controls both reads and writes.

```py
class Child(Base):
    __slots__ = ("value",)

item = Child()
reveal_type(item.value)  # revealed: int
item.value = 1
item.value = "wrong"  # error: [invalid-assignment]
```

A generic base class supplies the type chosen by its subclass.

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class GenericBase(Generic[T]):
    __slots__ = ()
    value: T

class IntegerChild(GenericBase[int]):
    __slots__ = ("value",)

reveal_type(IntegerChild().value)  # revealed: int
IntegerChild().value = "wrong"  # error: [invalid-assignment]
```

## Subclass annotations override inherited slot types

A subclass can narrow an inherited attribute declaration even when its storage remains in a base
class's slot. Reads and writes use the subclass's declared type, as they do without slots.

```py
class Base:
    __slots__ = ("value",)
    value: int | None

class Child(Base):
    __slots__ = ()
    value: int

reveal_type(Child().value)  # revealed: int
Child().value = 2
Child().value = None  # error: [invalid-assignment]
```

An annotation on an assignment in the subclass's initializer establishes the same narrower type.

```py
class InitializedChild(Base):
    def __init__(self) -> None:
        self.value: int = 1

    def get(self) -> int:
        return self.value

reveal_type(InitializedChild().value)  # revealed: int
InitializedChild().value = None  # error: [invalid-assignment]
```

As with an ordinary instance attribute, an overriding annotation replaces the inherited type.

```py
class StringChild(Base):
    def __init__(self) -> None:
        self.value: str = "valid"

reveal_type(StringChild().value)  # revealed: str
StringChild().value = 1  # error: [invalid-assignment]
```

## Extra instance attributes require an instance dictionary

An instance without a dictionary cannot create attributes outside its declared slots.

```py
class Slotted:
    __slots__ = ("value",)
    shared = 1

    def __init__(self) -> None:
        self.value = 1
        self.extra = 2  # error: [unresolved-attribute]

Slotted().other = 3  # error: [unresolved-attribute]
Slotted().shared = 3  # error: [unresolved-attribute]
```

An explicit `__dict__` slot restores support for additional instance attributes.

```py
class WithDictionary:
    __slots__ = ("value", "__dict__")

    def __init__(self) -> None:
        self.extra = 1

reveal_type(WithDictionary().extra)  # revealed: int
```

An ordinary base class can also supply an inherited instance dictionary.

```py
class OrdinaryBase:
    pass

class InheritedDictionary(OrdinaryBase):
    __slots__ = ("value",)

    def __init__(self) -> None:
        self.extra = 1

reveal_type(InheritedDictionary().extra)  # revealed: int
```

A subclass without its own `__slots__` regains an instance dictionary.

```py
class SlottedBase:
    __slots__ = ("value",)

class OrdinaryChild(SlottedBase):
    def __init__(self) -> None:
        self.extra = 1

reveal_type(OrdinaryChild().extra)  # revealed: int
```

## Dataclass-generated slots

A dataclass with `slots=True` does not give its instances a dictionary.

```py
from dataclasses import dataclass

@dataclass(slots=True)
class SlottedDataclass:
    value: int

SlottedDataclass(1).extra = 1  # error: [unresolved-attribute]
```

Its subclasses inherit that restricted instance layout unless they introduce a dictionary.

```py
class SlottedChild(SlottedDataclass):
    __slots__ = ("other",)

    def initialize(self) -> None:
        self.extra = 1  # error: [unresolved-attribute]
```

## Dataclass-generated slots exclude inherited slots

A slotted dataclass creates descriptors only for fields that do not already have an inherited slot.

```py
from dataclasses import dataclass

@dataclass(slots=True)
class Parent:
    value: int

@dataclass(slots=True)
class Child(Parent):
    other: int

reveal_type(Child.__slots__)  # revealed: tuple[Literal["other"]]
```

Redeclaring an inherited field does not create a second slot for that field.

```py
@dataclass(slots=True)
class Redefined(Parent):
    value: int
    other: int

reveal_type(Redefined.__slots__)  # revealed: tuple[Literal["other"]]
```

An inherited field still needs a new slot when its original class stored the field in an instance
dictionary.

```py
@dataclass
class UnslottedParent:
    value: int

@dataclass(slots=True)
class SlottedChild(UnslottedParent):
    other: int

reveal_type(SlottedChild.__slots__)  # revealed: tuple[Literal["value"], Literal["other"]]
```

An ordinary slotted base also supplies storage for any matching dataclass field.

```py
class SlottedBase:
    __slots__ = ("value",)

@dataclass(slots=True)
class SlottedChild(SlottedBase):
    value: int
    other: int

reveal_type(SlottedChild.__slots__)  # revealed: tuple[Literal["other"]]
```

## Dataclass-generated slots on Python 3.10

Python 3.10 includes inherited fields in generated dataclass slots. For consistency across Python
versions, ty intentionally uses the Python 3.11-and-later behavior when targeting Python 3.10.

```toml
[environment]
python-version = "3.10"
```

```py
from dataclasses import dataclass

@dataclass(slots=True)
class Parent:
    value: int

@dataclass(slots=True)
class Child(Parent):
    other: int

reveal_type(Child.__slots__)  # revealed: tuple[Literal["other"]]
```

## Slots generated by dataclass transforms

A dataclass-like decorator can also generate slots. The resulting class has the same restricted
instance layout as an ordinary slotted dataclass.

```py
from typing import Callable, TypeVar
from typing_extensions import dataclass_transform

T = TypeVar("T", bound=type)

@dataclass_transform()
def model(*, slots: bool = False) -> Callable[[T], T]:
    raise NotImplementedError

@model(slots=True)
class SlottedModel:
    value: int

SlottedModel(1).other = 1  # error: [unresolved-attribute]
```

## Slotted subclasses of built-in types

A slotted subclass of a built-in type without an instance dictionary cannot create extra attributes.

```py
class SlottedString(str):
    __slots__ = ("value",)

    def initialize(self) -> None:
        self.extra = 1  # error: [unresolved-attribute]
```

## Built-in bases with instance dictionaries

`staticmethod` instances have dictionaries, so a slotted subclass can still create additional
attributes.

```py
from typing import Any

class SlottedStaticMethod(staticmethod[..., Any]):
    __slots__ = ("value",)

    def __init__(self) -> None:
        super().__init__(lambda: 1)
        self.extra = 1
```

`classmethod` instances also have dictionaries.

```py
class SlottedClassMethod(classmethod[Any, ..., Any]):
    __slots__ = ("value",)

    def __init__(self) -> None:
        super().__init__(lambda cls: 1)
        self.extra = 1
```

## Descriptor setters do not require instance dictionaries

A data descriptor can accept assignments even when its owning instance has no dictionary.

```py
from typing import Any

class Descriptor:
    def __set__(self, instance: object, value: int) -> None: ...

class SlottedDescriptor:
    __slots__ = ()
    value = Descriptor()

SlottedDescriptor().value = 1
SlottedDescriptor().value = "wrong"  # error: [invalid-assignment]
```

Annotating a descriptor as `Any` does not hide the setter defined by the actual descriptor.

```py
class AnnotatedDescriptor:
    __slots__ = ()
    value: Any = Descriptor()

AnnotatedDescriptor().value = 1
```

## Custom attribute setters do not require instance dictionaries

A custom `__setattr__` method can decide how assignments are handled even when its instances have no
dictionaries.

```py
class CustomSetter:
    __slots__ = ()
    shared = 1

    def __setattr__(self, name: str, value: int) -> None: ...

CustomSetter().shared = 1
```

## Instance dictionaries and inherited annotations

Typeshed declares `__dict__` on `object`, so the attribute remains available through ordinary
attribute lookup even when a slotted instance has no dictionary at runtime.

```py
class Slotted:
    __slots__ = ("value",)

reveal_type(Slotted().__dict__)  # revealed: dict[str, Any]
reveal_type(Slotted.__dict__)  # revealed: dict[str, Any]
```

An unslotted subclass can introduce an instance dictionary, so methods on a slotted base may access
the dictionary after checking whether it exists.

```py
from typing import Any

class SlottedBase:
    __slots__ = ()

    def attributes(self) -> dict[str, Any]:
        if hasattr(self, "__dict__"):
            return self.__dict__
        return {}

class OrdinaryChild(SlottedBase):
    pass

reveal_type(OrdinaryChild().__dict__)  # revealed: dict[str, Any]
```

## Weak-reference slots create descriptors

A slotted instance does not expose `__weakref__` unless the slot is explicitly declared.

```py
class Slotted:
    __slots__ = ("value",)

Slotted().__weakref__  # error: [unresolved-attribute]
```

An explicit `__weakref__` slot permits reads on the class and its instances. The typeshed descriptor
returns `Any` for both forms of access.

```py
class WithWeakReference:
    __slots__ = ("value", "__weakref__")

reveal_type(WithWeakReference.__weakref__)  # revealed: Any
reveal_type(WithWeakReference().__weakref__)  # revealed: Any
```

The typeshed descriptor permits writing and deleting, so the runtime restriction on weak-reference
storage is not modeled.

```py
WithWeakReference().__weakref__ = None
del WithWeakReference().__weakref__
```

## A property named `__dict__` does not provide instance storage

A slotted class can expose a property named `__dict__` without acquiring ordinary instance
dictionary storage.

```py
class VirtualDictionary:
    __slots__ = ()

    @property
    def __dict__(self) -> dict[str, int]:
        return {"virtual": 1}

reveal_type(VirtualDictionary().__dict__)  # revealed: dict[str, int]
VirtualDictionary().extra = 1  # error: [unresolved-attribute]
```

## Weak-reference storage inherited from ordinary bases

Ordinary classes provide weak-reference storage at runtime, but their implicit `__weakref__`
attributes are not modeled. The same limitation applies to slotted subclasses.

```toml
[environment]
python-version = "3.11"
```

```py
class OrdinaryBase:
    pass

class SlottedChild(OrdinaryBase):
    __slots__ = ("value",)

OrdinaryBase().__weakref__  # error: [unresolved-attribute]
SlottedChild().__weakref__  # error: [unresolved-attribute]
```

Without modeling that inherited storage, a slotted dataclass also includes a requested
weak-reference slot even though the ordinary base already provides it at runtime.

```py
from dataclasses import dataclass

@dataclass(slots=True, weakref_slot=True)
class SlottedDataclass(OrdinaryBase):
    value: int

reveal_type(SlottedDataclass.__slots__)  # revealed: tuple[Literal["value"], Literal["__weakref__"]]
```

## Class-body annotations do not require instance storage

A bare annotation does not require an instance slot. It also does not make the attribute writable
without a slot.

```py
class Slotted:
    __slots__ = ("value",)
    value: int
    missing: int

Slotted().missing = 1  # error: [unresolved-attribute]
```

## Class attributes cannot have the same name as a slot

Assigning to a slot name in the class body prevents Python from creating the class.

```py
class Conflicting:
    __slots__ = ("value",)
    value = 1  # error: [invalid-assignment]
```

A method with the same name also occupies the final class namespace and conflicts with the slot.

```py
class ConflictingMethod:
    __slots__ = ("value",)

    def value(self) -> None:  # error: [invalid-assignment]
        pass
```

A temporary class variable that is deleted before the class is created does not conflict.

```py
class DeletedDefault:
    __slots__ = ("value",)
    value = 1
    del value
```

Class assignments inside `TYPE_CHECKING` blocks do not execute and therefore cannot conflict with
runtime slot descriptors.

```py
from typing import TYPE_CHECKING, ClassVar

class TypeCheckingOnly:
    __slots__ = ("value",)

    if TYPE_CHECKING:
        value: ClassVar[int] = 1
```
