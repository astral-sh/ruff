# Descriptor protocol

[Descriptors] let objects customize attribute lookup, storage, and deletion.

A descriptor is an attribute value that has one of the methods in the descriptor protocol. Those
methods are `__get__()`, `__set__()`, and `__delete__()`. If any of those methods are defined for an
attribute, it is said to be a descriptor.

## Basic properties

### Example

An introductory example, modeled after a [simple example] in the primer on descriptors, involving a
descriptor that returns a constant value:

```py
from typing import Literal

class Ten:
    def __get__(self, instance: object, owner: type | None = None) -> Literal[10]:
        return 10

    def __set__(self, instance: object, value: Literal[10]) -> None:
        pass

class C:
    ten: Ten = Ten()

c = C()

reveal_type(c.ten)  # revealed: Literal[10]

reveal_type(C.ten)  # revealed: Literal[10]

# This is fine:
c.ten = 10

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `ten` on type `C`"
c.ten = 11
```

When assigning to the `ten` attribute from the class object, we get an error. The descriptor
protocol is *not* triggered in this case. Since the attribute is declared as `Ten` in the class
body, we do not allow these assignments, preventing users from accidentally overwriting the data
descriptor, which is what would happen at runtime:

```py
# error: [invalid-assignment] "Object of type `Literal[10]` is not assignable to attribute `ten` of type `Ten`"
C.ten = 10
# error: [invalid-assignment] "Object of type `Literal[11]` is not assignable to attribute `ten` of type `Ten`"
C.ten = 11
```

### Different types for `__get__` and `__set__`

The return type of `__get__` and the value type of `__set__` can be different:

```py
class FlexibleInt:
    def __init__(self):
        self._value: int | None = None

    def __get__(self, instance: object, owner: type | None = None) -> int | None:
        return self._value

    def __set__(self, instance: object, value: int | str) -> None:
        self._value = int(value)

class C:
    flexible_int: FlexibleInt = FlexibleInt()

c = C()

reveal_type(c.flexible_int)  # revealed: int | None

c.flexible_int = 42  # okay
c.flexible_int = "42"  # also okay!

reveal_type(c.flexible_int)  # revealed: int | None

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `flexible_int` on type `C`"
c.flexible_int = None  # not okay

reveal_type(c.flexible_int)  # revealed: int | None
```

### Enum complement as descriptor mutation receiver

`overloaded.pyi`:

```pyi
from enum import Enum
from typing import Literal, overload

class Marker:
    @overload
    def __set__(self, instance: "Literal[Color.GREEN]", value: int) -> None: ...
    @overload
    def __set__(self, instance: "Literal[Color.BLUE]", value: int) -> None: ...
    def __set__(self, instance, value: int) -> None: ...
    @overload
    def __delete__(self, instance: "Literal[Color.GREEN]") -> None: ...
    @overload
    def __delete__(self, instance: "Literal[Color.BLUE]") -> None: ...
    def __delete__(self, instance) -> None: ...

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

    marker: Marker
```

```py
from typing import Literal

from overloaded import Color

def narrowed(color: Color):
    if color is Color.RED:
        return
    color.marker = 1
    del color.marker

def explicit(color: Literal[Color.GREEN, Color.BLUE]):
    color.marker = 1
    del color.marker
```

### Data and non-data descriptors

Descriptors that define `__set__` or `__delete__` are called *data descriptors*. An example of a
data descriptor is a `property` with a setter and/or a deleter. Descriptors that only define
`__get__`, meanwhile, are called *non-data descriptors*. Examples include functions, `classmethod`
or `staticmethod`.

The precedence chain for attribute access is (1) data descriptors, (2) instance attributes, and (3)
non-data descriptors.

```py
from typing import Literal

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: object, value: int) -> None:
        pass

class NonDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["non-data"]:
        return "non-data"

class C:
    data_descriptor = DataDescriptor()
    non_data_descriptor = NonDataDescriptor()

    def f(self):
        # This explains why data descriptors come first in the precedence chain. If
        # instance attributes would take priority, we would override the descriptor
        # here. Instead, this calls `DataDescriptor.__set__`, i.e. it does not affect
        # the type of the `data_descriptor` attribute.
        self.data_descriptor = 1

        # However, for non-data descriptors, instance attributes do take precedence.
        # So it is possible to override them.
        # error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `non_data_descriptor` of type `NonDataDescriptor`"
        self.non_data_descriptor = 1

c = C()

reveal_type(c.data_descriptor)  # revealed: Literal["data"]

reveal_type(c.non_data_descriptor)  # revealed: Literal["non-data"] | int

reveal_type(C.data_descriptor)  # revealed: Literal["data"]

reveal_type(C.non_data_descriptor)  # revealed: Literal["non-data"]

# Assignments through class objects are still checked against the declared
# descriptor type.
# error: [invalid-assignment] "Object of type `Literal["something else"]` is not assignable to attribute `data_descriptor` of type `DataDescriptor`"
C.data_descriptor = "something else"
```

### Partial fall back

Our implementation of the descriptor protocol takes into account that symbols can be possibly
unbound. In those cases, we fall back to lower precedence steps of the descriptor protocol and union
all possible results accordingly. We start by defining a data and a non-data descriptor:

```py
from typing import Literal

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: object, value: int) -> None:
        pass

class NonDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["non-data"]:
        return "non-data"
```

Then, we demonstrate that we fall back to an instance attribute if a data descriptor is possibly
unbound:

```py
def f1(flag: bool):
    class C1:
        if flag:
            attr = DataDescriptor()

        def f(self):
            # error: [invalid-assignment] "Invalid assignment to data descriptor attribute `attr` on type `Self@f`"
            self.attr = b"foo"

    reveal_type(C1().attr)  # revealed: Literal["data"] | bytes

    # Assigning to the attribute also causes no `possibly-unbound` diagnostic:
    # error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `attr` of type `bytes`"
    C1().attr = 1
```

We never treat implicit instance attributes as definitely bound, so we fall back to the non-data
descriptor here:

```py
class C2:
    def f(self):
        # error: [invalid-assignment] "Object of type `Literal[b"normal"]` is not assignable to attribute `attr` of type `NonDataDescriptor`"
        self.attr = b"normal"
    attr = NonDataDescriptor()

reveal_type(C2().attr)  # revealed: Literal["non-data"] | bytes

# Reads still fall back to the instance attribute in this case, but assignments
# are checked against the declared class attribute type.
# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `attr` of type `NonDataDescriptor`"
C2().attr = 1
```

### Classes with unknown bases are not automatically descriptors

When we cannot determine a class's base, we treat that base as `Unknown`. A `__get__` method written
on the class still makes it a descriptor, but we do not assume that `Unknown` supplies `__set__` or
`__delete__`. A `__set__` method written on the class still makes it a data descriptor:

```py
from typing import Literal
from unknown_module import UnknownBase  # error: [unresolved-import]

class NotADescriptor(UnknownBase): ...

class NonDataDescriptor(UnknownBase):
    def __get__(self, instance: object, owner: type | None = None) -> Literal["non-data"]:
        return "non-data"

class DataDescriptor(UnknownBase):
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: object, value: object) -> None:
        pass

class C:
    plain: NotADescriptor
    non_data: NonDataDescriptor
    data: DataDescriptor

reveal_type(C().plain)  # revealed: NotADescriptor
reveal_type(C().non_data)  # revealed: Literal["non-data"] | NonDataDescriptor
reveal_type(C().data)  # revealed: Literal["data"]
```

An `Any` base follows the same rule. A `__get__` method written on the class still makes it a
descriptor, but we do not assume that `Any` supplies `__set__` or `__delete__`. A `__set__` method
written on the class still makes it a data descriptor:

```py
from typing import Any, Literal

class NotADescriptor(Any): ...

class NonDataDescriptor(Any):
    def __get__(self, instance: object, owner: type | None = None) -> Literal["non-data"]:
        return "non-data"

class DataDescriptor(Any):
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: object, value: object) -> None:
        pass

class C:
    plain: NotADescriptor
    non_data: NonDataDescriptor
    data: DataDescriptor

reveal_type(C().plain)  # revealed: NotADescriptor
reveal_type(C().non_data)  # revealed: Literal["non-data"] | NonDataDescriptor
reveal_type(C().data)  # revealed: Literal["data"]

class OptionalAttribute:
    value: NotADescriptor | None

optional_attribute = OptionalAttribute()
optional_attribute.value = NotADescriptor()

# The assignment can narrow because `NotADescriptor` has no concrete `__set__` or `__delete__`
# method.
reveal_type(optional_attribute.value)  # revealed: NotADescriptor
```

An `Any` base that appears before another base class may override that class's `__get__` method at
runtime. We still use the later method to determine that the class is a descriptor, but its return
type must account for the earlier `Any` base:

```py
class DynamicBase:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["dynamic"]:
        return "dynamic"

class ConcreteBase:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["concrete"]:
        return "concrete"

    def __set__(self, instance: object, value: object) -> None:
        pass

AnyBase: Any = DynamicBase

class Descriptor(AnyBase, ConcreteBase): ...

class DescriptorOwner:
    attribute: Descriptor = Descriptor()

reveal_type(DescriptorOwner().attribute)  # revealed: Literal["concrete"] & Any
```

### Dynamically typed metaclass attributes

A metaclass attribute typed as `Any` could itself be a data descriptor. When the attribute is read
on a class, it therefore takes precedence over a class attribute with the same name. The same
applies when `Any` is one arm of a union:

```py
from typing import Any, Literal

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["descriptor"]:
        return "descriptor"

    def __set__(self, instance: object, value: object) -> None:
        pass

class Meta(type):
    attribute: Any = DataDescriptor()

class C(metaclass=Meta):
    attribute: int = 1

reveal_type(C.attribute)  # revealed: Any
C.attribute = "could be accepted by the dynamic descriptor"

class UnionMeta(type):
    attribute: Any | DataDescriptor = DataDescriptor()

class UnionC(metaclass=UnionMeta):
    attribute: int = 1

reveal_type(UnionC.attribute)  # revealed: Any | Literal["descriptor"]
```

### `TypeForm` metaclass attributes

A `TypeForm` argument describes the instances produced by a type form, not the runtime type form
value itself. A metaclass attribute typed as `TypeForm[Descriptor]` can therefore be a class whose
own metaclass makes it a data descriptor, and must continue to take precedence over a class
attribute with the same name when assigning to the attribute:

```py
from typing_extensions import TypeForm

class DescriptorMeta(type):
    def __set__(self, instance: object, value: str) -> None:
        pass

class Descriptor(metaclass=DescriptorMeta): ...

class Meta(type):
    attribute: TypeForm[Descriptor] = Descriptor

class C(metaclass=Meta):
    attribute: int = 1

C.attribute = 1  # error: [invalid-assignment]
# error: [invalid-assignment]
C.attribute = Descriptor  # error: [invalid-assignment]
```

A quoted type expression remains valid when both possible write targets accept the same runtime
string:

```py
class StringC(metaclass=Meta):
    attribute: str = ""

StringC.attribute = "valid"
StringC.attribute = "Descriptor"
```

The descriptor setter still rejects a class object even when the fallback attribute accepts that
same class object:

```py
class TypeFormC(metaclass=Meta):
    attribute: TypeForm[Descriptor] = Descriptor

TypeFormC.attribute = Descriptor  # error: [invalid-assignment]
```

The same contextual check applies when the metaclass attribute can also hold an ordinary string:

```py
class UnionMeta(type):
    attribute: TypeForm[Descriptor] | str = Descriptor

class UnionC(metaclass=UnionMeta):
    attribute: int = 1

UnionC.attribute = 1  # error: [invalid-assignment]
```

### Bounded class-object metaclass attributes

An inexact `type[Base]` attribute can hold a subclass whose custom metaclass makes the class object
a data descriptor. It must therefore continue to take precedence over a class attribute with the
same name when assigning to the attribute:

```py
class Base: ...

class DescriptorMeta(type):
    def __set__(self, instance: object, value: str) -> None:
        pass

class Descriptor(Base, metaclass=DescriptorMeta): ...

class Meta(type):
    attribute: type[Base] = Descriptor

class C(metaclass=Meta):
    attribute: int = 1

C.attribute = 1  # error: [invalid-assignment]
# error: [invalid-assignment]
C.attribute = Descriptor  # error: [invalid-assignment]
```

An assignment succeeds when both the possible descriptor setter and class attribute accept the
assigned string:

```py
class StringC(metaclass=Meta):
    attribute: str = ""

StringC.attribute = "valid"
```

An assignment fails when the class attribute accepts the assigned class but the descriptor setter
does not:

```py
class ClassC(metaclass=Meta):
    attribute: type[Base] = Base

ClassC.attribute = Base  # error: [invalid-assignment]
```

### Broad class-object metaclass attributes

Both `type[object]` and bare `type` can contain a class whose metaclass implements `__set__`. Their
possible descriptor setters must therefore be checked independently of the class-attribute fallback:

```py
class Base: ...

class DescriptorMeta(type):
    def __set__(self, instance: object, value: str) -> None:
        pass

class Descriptor(Base, metaclass=DescriptorMeta): ...

class ObjectMeta(type):
    attribute: type[object] = Descriptor

class ObjectStringC(metaclass=ObjectMeta):
    attribute: str = ""

ObjectStringC.attribute = "valid"

class ObjectClassC(metaclass=ObjectMeta):
    attribute: type[Base] = Base

ObjectClassC.attribute = Base  # error: [invalid-assignment]
```

The unparameterized spelling follows the same descriptor and class-attribute paths:

```py
class BareMeta(type):
    attribute: type = Descriptor

class BareStringC(metaclass=BareMeta):
    attribute: str = ""

BareStringC.attribute = "valid"

class BareClassC(metaclass=BareMeta):
    attribute: type[Base] = Base

BareClassC.attribute = Base  # error: [invalid-assignment]
```

### Class objects with unknown metaclasses

A `type[Any]` value could contain a class whose metaclass implements the descriptor protocol. We
therefore preserve the possibility that an attribute typed as `type[Any]` is a data descriptor, both
when reading the attribute and after assigning to it:

```py
from typing import Any

class DescriptorMeta(type):
    def __get__(self, instance: object, owner: type | None = None) -> str:
        return "descriptor"

    def __set__(self, instance: object, value: object) -> None:
        pass

class Descriptor(metaclass=DescriptorMeta): ...

class C:
    attribute: type[Any] = Descriptor

c = C()
reveal_type(c.attribute)  # revealed: Any

c.attribute = int
reveal_type(c.attribute)  # revealed: Any
```

### Descriptors only work when used as class variables

Descriptors only work when used as class variables. When put in instances, they have no effect.

```py
from typing import Literal

class Ten:
    def __get__(self, instance: object, owner: type | None = None) -> Literal[10]:
        return 10

class C:
    def __init__(self):
        self.ten: Ten = Ten()

reveal_type(C().ten)  # revealed: Ten

C().ten = Ten()

# The instance attribute is declared as `Ten`, so this is an
# error: [invalid-assignment] "Object of type `Literal[10]` is not assignable to attribute `ten` of type `Ten`"
C().ten = 10
```

## Descriptor protocol for class objects

When attributes are accessed on a class object, the following [precedence chain] is used:

- Data descriptor on the metaclass
- Data or non-data descriptor on the class
- Class attribute
- Non-data descriptor on the metaclass
- Metaclass attribute

To verify this, we define a data and a non-data descriptor:

```py
from typing import Literal, Any

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: object, value: int) -> None:
        pass

class NonDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["non-data"]:
        return "non-data"
```

First, we make sure that the descriptors are correctly accessed when defined on the metaclass or the
class:

```py
class Meta1(type):
    meta_data_descriptor: DataDescriptor = DataDescriptor()
    meta_non_data_descriptor: NonDataDescriptor = NonDataDescriptor()

class C1(metaclass=Meta1):
    class_data_descriptor: DataDescriptor = DataDescriptor()
    class_non_data_descriptor: NonDataDescriptor = NonDataDescriptor()

reveal_type(C1.meta_data_descriptor)  # revealed: Literal["data"]
reveal_type(C1.meta_non_data_descriptor)  # revealed: Literal["non-data"]

reveal_type(C1.class_data_descriptor)  # revealed: Literal["data"]
reveal_type(C1.class_non_data_descriptor)  # revealed: Literal["non-data"]
```

Assignments to class object attribute only trigger the descriptor protocol if the data descriptor is
on the metaclass:

```py
C1.meta_data_descriptor = 1

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `meta_data_descriptor` on type `<class 'C1'>`"
C1.meta_data_descriptor = "invalid"
```

When writing to a class-level data descriptor from the class object itself, the descriptor protocol
is *not* triggered (this is in contrast to what happens when you read class-level descriptor
attributes!). So the following assignment does not call `__set__`. At runtime, the assignment would
overwrite the data descriptor, but the attribute is declared as `DataDescriptor` in the class body,
so we do not allow this:

```py
# error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to attribute `class_data_descriptor` of type `DataDescriptor`"
C1.class_data_descriptor = 1
```

We now demonstrate that a *metaclass data descriptor* takes precedence over all class-level
attributes:

```py
class Meta2(type):
    meta_data_descriptor1: DataDescriptor = DataDescriptor()
    meta_data_descriptor2: DataDescriptor = DataDescriptor()

class ClassLevelDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["class level data descriptor"]:
        return "class level data descriptor"

    def __set__(self, instance: object, value: str) -> None:
        pass

class C2(metaclass=Meta2):
    meta_data_descriptor1: Literal["value on class"] = "value on class"
    meta_data_descriptor2: ClassLevelDataDescriptor = ClassLevelDataDescriptor()

reveal_type(C2.meta_data_descriptor1)  # revealed: Literal["data"]
reveal_type(C2.meta_data_descriptor2)  # revealed: Literal["data"]

C2.meta_data_descriptor1 = 1
C2.meta_data_descriptor2 = 1

# error: [invalid-assignment]
C2.meta_data_descriptor1 = "invalid"
# error: [invalid-assignment]
C2.meta_data_descriptor2 = "invalid"
```

On the other hand, normal metaclass attributes and metaclass non-data descriptors are shadowed by
class-level attributes (descriptor or not):

```py
class Meta3(type):
    meta_attribute1: Literal["value on metaclass"] = "value on metaclass"
    meta_attribute2: Literal["value on metaclass"] = "value on metaclass"
    meta_non_data_descriptor1: NonDataDescriptor = NonDataDescriptor()
    meta_non_data_descriptor2: NonDataDescriptor = NonDataDescriptor()

class C3(metaclass=Meta3):
    meta_attribute1: Literal["value on class"] = "value on class"
    meta_attribute2: ClassLevelDataDescriptor = ClassLevelDataDescriptor()
    meta_non_data_descriptor1: Literal["value on class"] = "value on class"
    meta_non_data_descriptor2: ClassLevelDataDescriptor = ClassLevelDataDescriptor()

reveal_type(C3.meta_attribute1)  # revealed: Literal["value on class"]
reveal_type(C3.meta_attribute2)  # revealed: Literal["class level data descriptor"]
reveal_type(C3.meta_non_data_descriptor1)  # revealed: Literal["value on class"]
reveal_type(C3.meta_non_data_descriptor2)  # revealed: Literal["class level data descriptor"]

C3.meta_non_data_descriptor1 = "value on class"
# error: [invalid-assignment] "Object of type `Literal["invalid"]` is not assignable to attribute `meta_non_data_descriptor1` of type `Literal["value on class"]`"
C3.meta_non_data_descriptor1 = "invalid"
```

Finally, metaclass attributes and metaclass non-data descriptors are only accessible when they are
not shadowed by class-level attributes:

```py
class Meta4(type):
    meta_attribute: Literal["value on metaclass"] = "value on metaclass"
    meta_non_data_descriptor: NonDataDescriptor = NonDataDescriptor()

class C4(metaclass=Meta4): ...

reveal_type(C4.meta_attribute)  # revealed: Literal["value on metaclass"]
reveal_type(C4.meta_non_data_descriptor)  # revealed: Literal["non-data"]
```

When a metaclass data descriptor is possibly missing, we union the result type of its `__get__`
method with an underlying class level attribute, if present:

```py
def _(flag: bool):
    class Meta5(type):
        if flag:
            meta_data_descriptor1: DataDescriptor = DataDescriptor()
            meta_data_descriptor2: DataDescriptor = DataDescriptor()

    class C5(metaclass=Meta5):
        meta_data_descriptor1: Literal["value on class"] = "value on class"

    reveal_type(C5.meta_data_descriptor1)  # revealed: Literal["data", "value on class"]
    # error: [possibly-missing-attribute]
    reveal_type(C5.meta_data_descriptor2)  # revealed: Literal["data"]

    # TODO: We currently emit two diagnostics here, corresponding to the two states of `flag`. The diagnostics are not
    # wrong, but they could be subsumed under a higher-level diagnostic.

    # error: [invalid-assignment] "Invalid assignment to data descriptor attribute `meta_data_descriptor1` on type `<class 'C5'>`"
    # error: [invalid-assignment] "Object of type `None` is not assignable to attribute `meta_data_descriptor1` of type `Literal["value on class"]`"
    C5.meta_data_descriptor1 = None

    # error: [possibly-missing-attribute]
    C5.meta_data_descriptor2 = 1
```

When a class-level attribute is possibly missing, we union its (descriptor protocol) type with the
metaclass attribute (unless it's a data descriptor, which always takes precedence):

```py
from typing import Any

def _(flag: bool):
    class Meta6(type):
        attribute1: DataDescriptor = DataDescriptor()
        attribute2: NonDataDescriptor = NonDataDescriptor()
        attribute3: Literal["value on metaclass"] = "value on metaclass"

    class C6(metaclass=Meta6):
        if flag:
            attribute1: Literal["value on class"] = "value on class"
            attribute2: Literal["value on class"] = "value on class"
            attribute3: Literal["value on class"] = "value on class"
            attribute4: Literal["value on class"] = "value on class"

    reveal_type(C6.attribute1)  # revealed: Literal["data"]
    reveal_type(C6.attribute2)  # revealed: Literal["non-data", "value on class"]
    reveal_type(C6.attribute3)  # revealed: Literal["value on metaclass", "value on class"]
    # error: [possibly-missing-attribute]
    reveal_type(C6.attribute4)  # revealed: Literal["value on class"]
```

Finally, we can also have unions of various types of attributes:

```py
def _(flag: bool):
    class Meta7(type):
        if flag:
            union_of_metaclass_attributes: Literal[1] = 1
            union_of_metaclass_data_descriptor_and_attribute: DataDescriptor = DataDescriptor()
        else:
            union_of_metaclass_attributes: Literal[2] = 2
            union_of_metaclass_data_descriptor_and_attribute: Literal[2] = 2

    class C7(metaclass=Meta7):
        if flag:
            union_of_class_attributes: Literal[1] = 1
            union_of_class_data_descriptor_and_attribute: DataDescriptor = DataDescriptor()
        else:
            union_of_class_attributes: Literal[2] = 2
            union_of_class_data_descriptor_and_attribute: Literal[2] = 2

    reveal_type(C7.union_of_metaclass_attributes)  # revealed: Literal[1, 2]
    reveal_type(C7.union_of_metaclass_data_descriptor_and_attribute)  # revealed: Literal["data", 2]
    reveal_type(C7.union_of_class_attributes)  # revealed: Literal[1, 2]
    reveal_type(C7.union_of_class_data_descriptor_and_attribute)  # revealed: Literal["data", 2]

    C7.union_of_metaclass_attributes = 2 if flag else 1
    # TODO: https://github.com/astral-sh/ty/issues/1163
    # error: [invalid-assignment]
    C7.union_of_metaclass_data_descriptor_and_attribute = 2 if flag else 100
    C7.union_of_class_attributes = 2 if flag else 1
    C7.union_of_class_data_descriptor_and_attribute = 2 if flag else DataDescriptor()
```

## Descriptors distinguishing between class and instance access

Overloads can be used to distinguish between when a descriptor is accessed on a class object and
when it is accessed on an instance. A real-world example of this is the `__get__` method on
`types.FunctionType`.

```py
from typing_extensions import Literal, LiteralString, overload

class Descriptor:
    @overload
    def __get__(self, instance: None, owner: type, /) -> Literal["called on class object"]: ...
    @overload
    def __get__(self, instance: object, owner: type | None = None, /) -> Literal["called on instance"]: ...
    def __get__(self, instance, owner=None, /) -> LiteralString:
        if instance:
            return "called on instance"
        else:
            return "called on class object"

class C:
    d: Descriptor = Descriptor()

reveal_type(C.d)  # revealed: Literal["called on class object"]

reveal_type(C().d)  # revealed: Literal["called on instance"]
```

## Descriptor protocol for dunder methods

Dunder methods are always looked up on the meta-type. There is no instance fallback. This means that
an implicit dunder call on an instance-like object will not only look up the dunder method on the
class object, without considering instance attributes. And an implicit dunder call on a class object
will look up the dunder method on the metaclass, without considering class attributes.

```py
class SomeCallable:
    def __call__(self, x: int) -> str:
        return "a"

class Descriptor:
    def __get__(self, instance: object, owner: type | None = None) -> SomeCallable:
        return SomeCallable()

class B:
    __call__: Descriptor = Descriptor()

b_instance = B()
reveal_type(b_instance(1))  # revealed: str

b_instance("bla")  # error: [invalid-argument-type]
```

## Special descriptors

### Built-in `property` descriptor

The built-in `property` decorator creates a descriptor. The names for attribute reads/writes are
determined by the return type of the `name` method and the parameter type of the setter,
respectively.

```py
class C:
    _name: str | None = None

    @property
    def name(self) -> str:
        return self._name or "Unset"

    @name.setter
    def name(self, value: str | None) -> None:
        self._value = value

c = C()

reveal_type(c._name)  # revealed: str | None
reveal_type(c.name)  # revealed: str
reveal_type(C.name)  # revealed: property

c.name = "new"
c.name = None

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `name` on type `C`"
c.name = 42
```

### Overriding properties in subclasses

When a subclass overrides a property, accessing other inherited properties from within the
overriding property methods should still work correctly.

```py
class Base:
    _value: float = 0.0

    @property
    def value(self) -> float:
        return self._value

    @value.setter
    def value(self, v: float) -> None:
        self._value = v

    @property
    def other(self) -> float:
        return self.value

    @other.setter
    def other(self, v: float) -> None:
        self.value = v

class Derived(Base):
    @property
    def other(self) -> float:
        return self.value

    @other.setter
    def other(self, v: float) -> None:
        reveal_type(self.value)  # revealed: float
        self.value = v
```

### Properties with no setters

If a property has no setter, we emit a bespoke error message when a user attempts to set that
attribute, since this is a common error.

```py
class DontAssignToMe:
    @property
    def immutable(self): ...

# snapshot: invalid-assignment
DontAssignToMe().immutable = "the properties, they are a-changing"
```

```snapshot
error[invalid-assignment]: Cannot assign to read-only property `immutable` on object of type `DontAssignToMe`
 --> src/mdtest_snippet.py:6:1
  |
3 |     def immutable(self): ...
  |         --------- Property `DontAssignToMe.immutable` defined here with no setter
4 |
5 | # snapshot: invalid-assignment
6 | DontAssignToMe().immutable = "the properties, they are a-changing"
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^ Attempted assignment to `DontAssignToMe.immutable` here
```

### Built-in `classmethod` descriptor

Similarly to `property`, `classmethod` decorator creates an implicit descriptor that binds the first
argument to the class instead of the instance.

```py
class C:
    def __init__(self, value: str) -> None:
        self._name: str = value

    @classmethod
    def factory(cls, value: str) -> "C":
        return cls(value)

    @classmethod
    def get_name(cls) -> str:
        return cls.__name__

c1 = C.factory("test")  # okay

reveal_type(c1)  # revealed: C

reveal_type(C.get_name())  # revealed: str

reveal_type(C("42").get_name())  # revealed: str
```

### Built-in `staticmethod` descriptor

```py
class C:
    @staticmethod
    def helper(value: str) -> str:
        return value

reveal_type(C.helper("42"))  # revealed: str
c = C()
reveal_type(c.helper("string"))  # revealed: str
```

### Functions as descriptors

Functions are descriptors because they implement a `__get__` method. This is crucial in making sure
that method calls work as expected. See [this test suite](./call/methods.md) for more information.
Here, we only demonstrate how `__get__` works on functions:

```py
import types
from inspect import getattr_static
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of

def f(x: object) -> str:
    return "a"

reveal_type(f)  # revealed: def f(x: object) -> str
reveal_type(f.__get__)  # revealed: <method-wrapper '__get__' of function 'f'>
static_assert(is_subtype_of(TypeOf[f.__get__], types.MethodWrapperType))
reveal_type(f.__get__(None, type(f)))  # revealed: def f(x: object) -> str
reveal_type(f.__get__(None, type(f))(1))  # revealed: str

wrapper_descriptor = getattr_static(f, "__get__")

reveal_type(wrapper_descriptor)  # revealed: <wrapper-descriptor '__get__' of 'function' objects>
reveal_type(wrapper_descriptor(f, None, type(f)))  # revealed: def f(x: object) -> str
static_assert(is_subtype_of(TypeOf[wrapper_descriptor], types.WrapperDescriptorType))

# Attribute access on the method-wrapper `f.__get__` falls back to `MethodWrapperType`:
reveal_type(f.__get__.__hash__)  # revealed: bound method MethodWrapperType.__hash__() -> int

# Attribute access on the wrapper-descriptor falls back to `WrapperDescriptorType`:
reveal_type(wrapper_descriptor.__qualname__)  # revealed: str
```

We can also bind the free function `f` to an instance of a class `C`:

```py
class C: ...

bound_method = wrapper_descriptor(f, C(), C)

reveal_type(bound_method)  # revealed: bound method C.f() -> str
```

We can then call it, and the instance of `C` is implicitly passed to the first parameter of `f`
(`x`):

```py
reveal_type(bound_method())  # revealed: str
```

Finally, we test some error cases for the call to the wrapper descriptor:

```py
# Calling the wrapper descriptor without any arguments is an
# error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
wrapper_descriptor()

# Calling it without the `instance` argument is an also an
# error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
wrapper_descriptor(f)

# TODO: Calling it without the `owner` argument if `instance` is not `None` fails at runtime.
# Ideally we would emit a diagnostic here,
# but this is hard to model without introducing false positives elsewhere
wrapper_descriptor(f, None)

# But calling it with an instance is fine (in this case, the `owner` argument is optional):
wrapper_descriptor(f, C())

# Calling it with something that is not a `FunctionType` as the first argument is an
# error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
wrapper_descriptor(1, None, type(f))

# Calling it with something that is not a `type` as the `owner` argument is an
# error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
wrapper_descriptor(f, None, f)

# Calling it with too many positional arguments is an
# error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
wrapper_descriptor(f, None, type(f), "one too many")
```

## Error handling and edge cases

### `__get__` is called with correct arguments

Python passes the instance and its class to a descriptor on an instance access. On a class access,
it passes `None` and the class instead. A descriptor on a metaclass receives the class and its
metaclass.

```py
from __future__ import annotations

class TailoredForClassObjectAccess:
    def __get__(self, instance: None, owner: type[C]) -> int:
        return 1

class TailoredForInstanceAccess:
    def __get__(self, instance: C, owner: type[C] | None = None) -> str:
        return "a"

class TailoredForMetaclassAccess:
    def __get__(self, instance: type[C], owner: type[Meta]) -> bytes:
        return b"a"

class Meta(type):
    metaclass_access: TailoredForMetaclassAccess = TailoredForMetaclassAccess()

class C(metaclass=Meta):
    class_object_access: TailoredForClassObjectAccess = TailoredForClassObjectAccess()
    instance_access: TailoredForInstanceAccess = TailoredForInstanceAccess()

reveal_type(C.class_object_access)  # revealed: int
reveal_type(C().instance_access)  # revealed: str
reveal_type(C.metaclass_access)  # revealed: bytes
```

An invalid descriptor access is reported, but we still use the declared return type of `__get__` to
avoid cascading errors.

```py
# snapshot: invalid-attribute-access
reveal_type(C().class_object_access)  # revealed: int

# snapshot: invalid-attribute-access
reveal_type(C.instance_access)  # revealed: str
```

```snapshot
error[invalid-attribute-access]: Invalid access to descriptor attribute `class_object_access` on type `C`
  --> src/mdtest_snippet.py:26:13
   |
26 | reveal_type(C().class_object_access)  # revealed: int
   |             ^^^ Expected `None`, found `C`
info: Argument to function `TailoredForClassObjectAccess.__get__` is incorrect
info: This access implicitly calls `__get__` on a descriptor of type `TailoredForClassObjectAccess`
info: Function defined here
 --> src/mdtest_snippet.py:4:9
  |
4 |     def __get__(self, instance: None, owner: type[C]) -> int:
  |         ^^^^^^^       -------------- Parameter declared here


error[invalid-attribute-access]: Invalid access to descriptor attribute `instance_access` on type `<class 'C'>`
  --> src/mdtest_snippet.py:29:13
   |
29 | reveal_type(C.instance_access)  # revealed: str
   |             ^ Expected `C`, found `None`
info: Argument to function `TailoredForInstanceAccess.__get__` is incorrect
info: This access implicitly calls `__get__` on a descriptor of type `TailoredForInstanceAccess`
info: Function defined here
 --> src/mdtest_snippet.py:8:9
  |
8 |     def __get__(self, instance: C, owner: type[C] | None = None) -> str:
  |         ^^^^^^^       ----------- Parameter declared here
```

### Descriptors with an incorrect `__get__` signature

Python calls `__get__` with the descriptor, an instance or `None`, and the owner class. A method
that accepts only the descriptor cannot handle that call.

```py
class Descriptor:
    # `__get__` method with missing parameters:
    def __get__(self) -> int:
        return 1

class C:
    descriptor: Descriptor = Descriptor()

C().descriptor  # snapshot: invalid-attribute-access

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `descriptor` on type `<class 'C'>`"
reveal_type(C.descriptor)  # revealed: int
```

```snapshot
error[invalid-attribute-access]: Invalid access to descriptor attribute `descriptor` on type `C`
 --> src/mdtest_snippet.py:9:1
  |
9 | C().descriptor  # snapshot: invalid-attribute-access
  | ^^^ Too many positional arguments to function `Descriptor.__get__`: expected 1, got 3
info: This access implicitly calls `__get__` on a descriptor of type `Descriptor`
info: Function signature here
 --> src/mdtest_snippet.py:3:9
  |
3 |     def __get__(self) -> int:
  |         ^^^^^^^^^^^^^^^^^^^^
```

### Recursive descriptor aliases terminate

Inspecting a recursive attribute must not recurse forever. The recursive alternative also cannot
prove that the access will invoke an invalid descriptor.

```toml
[environment]
python-version = "3.12"
```

```py
type Recursive = int | Recursive

class C:
    value: Recursive = 1

C().value
```

### Property getters reject invalid receiver specializations

A property getter checks the same specialized receiver as an ordinary method. A generic alias with
alternatives that impose different type-variable bounds can produce an invalid property access.

```py
from collections.abc import Callable
from typing import Generic, TypeVar

AItem = TypeVar("AItem", bound=Callable[[int], str])
BItem = TypeVar("BItem", bound=Callable[[str], str])

class A(Generic[AItem]):
    @property
    def callback(self) -> AItem:
        raise NotImplementedError

class B(Generic[BItem]):
    @property
    def callback(self) -> BItem:
        raise NotImplementedError

AnyCallback = TypeVar("AnyCallback", bound=Callable[..., str])
Command = A[AnyCallback] | B[AnyCallback]
Callback = TypeVar("Callback", bound=Callable[[int], str])

def access(value: Callback | Command[Callback]) -> None:
    if isinstance(value, A | B):
        # error: [invalid-attribute-access]
        value.callback
```

### Property getter failures preserve their underlying error and return type

A property inherited from an unrelated class rejects the instance passed to its getter. The
diagnostic reports the getter's actual receiver mismatch and preserves its return type.

```py
class Owner:
    @property
    def value(self) -> int:
        return 1

class Other:
    value = Owner.value

# error: [invalid-attribute-access] "Expected `Owner`, found `Other`"
reveal_type(Other().value)  # revealed: int
```

### Every descriptor alternative must accept the call

As with other operations on a union, an attribute access is invalid if any possible descriptor
cannot accept the implicit call.

```py
class BrokenDescriptor:
    def __get__(self) -> bytes:
        return b""

class ValidDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> str:
        return ""

def descriptor() -> BrokenDescriptor | ValidDescriptor:
    raise NotImplementedError

class C:
    value = descriptor()

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `C`"
reveal_type(C().value)  # revealed: bytes | str
```

### Descriptor diagnostics are reported through `super()`

Accessing an inherited descriptor through `super()` still invokes its `__get__` method.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class Base:
    value = Descriptor()

class Derived(Base):
    def access(self) -> None:
        # error: [invalid-attribute-access]
        super().value
```

### Type variables preserve invalid descriptor calls

A type variable's bound does not prevent its receiver or descriptor value from reaching an invalid
`__get__` method. The same applies when accessing an attribute on `type[T]`.

```py
from typing import TypeVar

class Descriptor:
    def __get__(self) -> int:
        return 1

class Owner:
    value = Descriptor()

OwnerT = TypeVar("OwnerT", bound=Owner)
DescriptorT = TypeVar("DescriptorT", bound=Descriptor)

def instance(owner: OwnerT) -> None:
    # error: [invalid-attribute-access]
    owner.value

def class_object(owner: type[OwnerT]) -> None:
    # error: [invalid-attribute-access]
    owner.value

def descriptor_value(descriptor: DescriptorT) -> None:
    class C:
        value = descriptor

    # error: [invalid-attribute-access]
    C().value
```

### Intersections preserve invalid descriptor calls

Intersecting a receiver or descriptor value with another type does not make its invalid `__get__`
method callable.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class Owner:
    value = Descriptor()

class Marker: ...

def receiver(owner: Owner) -> None:
    if isinstance(owner, Marker):
        # error: [invalid-attribute-access]
        owner.value

def descriptor_value(descriptor: Descriptor) -> None:
    if isinstance(descriptor, Marker):
        class C:
            value = descriptor

        # error: [invalid-attribute-access]
        C().value
```

### Every `__get__` definition must accept the call

A conditionally defined method can have several callable signatures. The access is invalid if any
possible definition rejects the call.

```py
def access(flag: bool) -> None:
    class Descriptor:
        if flag:
            def __get__(self, instance: object, owner: type | None = None) -> int:
                return 1

        else:
            def __get__(self) -> str:
                return ""

    class C:
        value = Descriptor()

    # error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `C`"
    reveal_type(C().value)  # revealed: int | str
```

### A possible `__getattr__` fallback does not hide an invalid descriptor

When a descriptor is only conditionally present, `__getattr__` handles the path where it is absent.
The other path still invokes the invalid descriptor and must produce a diagnostic.

```py
def access(flag: bool) -> None:
    class Descriptor:
        def __get__(self) -> int:
            return 1

    class C:
        if flag:
            value = Descriptor()

        def __getattr__(self, name: str) -> str:
            return name

    # error: [invalid-attribute-access]
    reveal_type(C().value)  # revealed: int | str
```

### A class-object lookup uses its declared member type

Class-object member lookup uses the declared attribute type even when the declaration has no value.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class C:
    value: Descriptor

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `<class 'C'>`"
C.value
```

### An instance `__getattribute__` can bypass descriptors

A custom `__getattribute__` can return without invoking the malformed descriptor. The ordinary
member type remains unchanged, even when the override has the same return type as the descriptor.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class C:
    value = Descriptor()

    def __getattribute__(self, name: str) -> int:
        return 42

reveal_type(C().value)  # revealed: int
```

### An unknown `__getattribute__` can bypass descriptors

A dynamic base may provide an attribute interceptor that avoids a malformed descriptor, so the
descriptor access cannot be guaranteed to fail.

```py
from typing import Any

class Descriptor:
    def __get__(self) -> int:
        return 1

class C(Any):
    value = Descriptor()

reveal_type(C().value)  # revealed: int
```

### An instance `__getattribute__` may delegate to descriptor lookup

The return annotation of an override does not establish whether it delegates to the default
attribute lookup. Since ty does not inspect the implementation, it cannot conclude that the
descriptor is invoked.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class C:
    value = Descriptor()

    def __getattribute__(self, name: str) -> str:
        return super().__getattribute__(name)

C().value
```

### An invalid `__getattribute__` runs before descriptors

A malformed `__getattribute__` fails before it can invoke a malformed descriptor. The diagnostic
therefore describes the `__getattribute__` call while preserving the descriptor's return type.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class C:
    value = Descriptor()

    # error: [invalid-method-override]
    def __getattribute__(self) -> str:
        return "fallback"

# error: [invalid-attribute-access] "Invalid access to attribute `value` on type `C`"
reveal_type(C().value)  # revealed: int
```

### An assigned instance attribute shadows a non-data descriptor

An instance attribute takes precedence over a non-data descriptor. After the assignment, reading the
attribute does not call the descriptor.

```py
from typing import Literal

class Descriptor:
    def __get__(self) -> str:
        return ""

class C:
    value = Descriptor()

    def replace(self) -> None:
        self.value: int = 1
        reveal_type(self.value)  # revealed: Literal[1]
```

### An instance assignment does not shadow a data descriptor

Assigning to a data descriptor invokes its `__set__` method. A subsequent read still invokes its
`__get__` method, even though the attribute has a known assigned type.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

    def __set__(self, instance: object, value: int) -> None:
        pass

class C:
    value = Descriptor()

    def access(self) -> None:
        self.value = 1
        # error: [invalid-attribute-access]
        self.value
```

### A conditional assignment does not hide an invalid descriptor call

The assignment shadows the non-data descriptor on one path, but the other path still invokes its
invalid `__get__` method.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

class C:
    value = Descriptor()

def access(c: C, flag: bool) -> None:
    if flag:
        c.value = Descriptor()

    # error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `C`"
    c.value
```

### Augmented assignment reads before writing

An augmented assignment reads the descriptor before writing the operation's result. The malformed
`__get__` call is therefore reported even though `__set__` accepts the result.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

    def __set__(self, instance: object, value: int) -> None:
        pass

class C:
    value = Descriptor()

c = C()
# error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `C`"
c.value += 1
```

### Deletion does not read a descriptor

Deleting a descriptor calls `__delete__` without first calling `__get__`.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

    def __delete__(self, instance: object) -> None:
        pass

class C:
    value = Descriptor()

c = C()
del c.value
```

### A class attribute can shadow a metaclass non-data descriptor

The class attribute takes precedence, so the malformed metaclass descriptor is not invoked.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

class Meta(type):
    value = Descriptor()

class C(metaclass=Meta):
    value = 1

reveal_type(C.value)  # revealed: int
```

### A possible class attribute does not shadow a metaclass descriptor

A conditionally defined class attribute shadows a metaclass descriptor only when it exists. The
other path invokes the invalid descriptor.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

class Meta(type):
    value = Descriptor()

def access(flag: bool) -> None:
    class C(metaclass=Meta):
        if flag:
            value = 1

    # error: [invalid-attribute-access]
    reveal_type(C.value)  # revealed: str | int
```

### A metaclass data descriptor takes precedence over a class attribute

A data descriptor on the metaclass runs even when the class defines an attribute with the same name,
so an invalid descriptor call must be reported.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

    def __set__(self, instance: object, value: int) -> None:
        pass

class Meta(type):
    value = Descriptor()

class C(metaclass=Meta):
    value = 1

# error: [invalid-attribute-access]
reveal_type(C.value)  # revealed: str
```

### A metaclass data descriptor shadows an invalid class descriptor

A data descriptor on the metaclass has priority over a descriptor stored on the class. The class
descriptor is never called, so its invalid signature does not affect the access.

```py
class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: object, value: int) -> None:
        pass

class InvalidDescriptor:
    def __get__(self) -> str:
        return ""

class Meta(type):
    value = DataDescriptor()

class C(metaclass=Meta):
    value = InvalidDescriptor()

reveal_type(C.value)  # revealed: int
```

### `__get__` is not callable

Python still attempts to call a non-callable `__get__` attribute, so the access fails and its type
is unknown.

```py
class Descriptor:
    __get__: None = None

class C:
    value: Descriptor = Descriptor()

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `C`"
reveal_type(C().value)  # revealed: Unknown
```

### Undeclared descriptor arguments

If a descriptor attribute is not declared, we union with `Unknown`, just like for regular
attributes, since that attribute could be overwritten externally. Even a data descriptor with a
`__set__` method can be overwritten when accessed through a class object.

```py
class Descriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: object, value: int) -> None:
        pass

class C:
    descriptor = Descriptor()

# error: [invalid-assignment] "Object of type `Literal["something else"]` is not assignable to attribute `descriptor` of type `Descriptor`"
C.descriptor = "something else"
reveal_type(C.descriptor)  # revealed: int
```

### Possibly unbound descriptor attributes

```py
class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

    def __set__(self, instance: int, value) -> None:
        pass

class NonDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

def _(flag: bool):
    class PossiblyUnbound:
        if flag:
            non_data: NonDataDescriptor = NonDataDescriptor()
            data: DataDescriptor = DataDescriptor()

    # error: [possibly-missing-attribute] "Attribute `non_data` may be missing on class `PossiblyUnbound`"
    reveal_type(PossiblyUnbound.non_data)  # revealed: int

    # error: [possibly-missing-attribute] "Attribute `non_data` may be missing on object of type `PossiblyUnbound`"
    reveal_type(PossiblyUnbound().non_data)  # revealed: int

    # error: [possibly-missing-attribute] "Attribute `data` may be missing on class `PossiblyUnbound`"
    reveal_type(PossiblyUnbound.data)  # revealed: int

    # error: [possibly-missing-attribute] "Attribute `data` may be missing on object of type `PossiblyUnbound`"
    reveal_type(PossiblyUnbound().data)  # revealed: int
```

### Possibly-unbound `__get__` method

```py
def _(flag: bool):
    class MaybeDescriptor:
        if flag:
            def __get__(self, instance: object, owner: type | None = None) -> int:
                return 1

    class C:
        descriptor: MaybeDescriptor = MaybeDescriptor()

    reveal_type(C.descriptor)  # revealed: int | MaybeDescriptor

    reveal_type(C().descriptor)  # revealed: int | MaybeDescriptor
```

### A possibly-unbound invalid `__get__` method still fails when present

When a descriptor method is only conditionally defined, the branch where it exists must still accept
the implicit descriptor arguments.

```py
def access(flag: bool) -> None:
    class Descriptor:
        if flag:
            def __get__(self) -> int:
                return 1

    class C:
        value = Descriptor()

    # error: [invalid-attribute-access]
    reveal_type(C().value)  # revealed: int | Descriptor
```

### Descriptors with non-function `__get__` callables that are descriptors themselves

The descriptor protocol is recursive, i.e. looking up `__get__` can involve triggering the
descriptor protocol on the callable's `__call__` method:

```py
from __future__ import annotations

class ReturnedCallable2:
    def __call__(self, descriptor: Descriptor1, instance: None, owner: type[C]) -> int:
        return 1

class ReturnedCallable1:
    def __call__(self, descriptor: Descriptor2, instance: Callable1, owner: type[Callable1]) -> ReturnedCallable2:
        return ReturnedCallable2()

class Callable3:
    def __call__(self, descriptor: Descriptor3, instance: Callable2, owner: type[Callable2]) -> ReturnedCallable1:
        return ReturnedCallable1()

class Descriptor3:
    __get__: Callable3 = Callable3()

class Callable2:
    __call__: Descriptor3 = Descriptor3()

class Descriptor2:
    __get__: Callable2 = Callable2()

class Callable1:
    __call__: Descriptor2 = Descriptor2()

class Descriptor1:
    __get__: Callable1 = Callable1()

class C:
    d: Descriptor1 = Descriptor1()

reveal_type(C.d)  # revealed: int
```

### Descriptors with `Concatenate` self-types on `__get__`

This is a regression test for <https://github.com/astral-sh/ty/issues/3289>.

```py
from typing import Any, Callable, Concatenate, Generic, ParamSpec, TypeVar

P = ParamSpec("P")
P2 = ParamSpec("P2")
T = TypeVar("T")

class FunctionWrapper(Generic[P]):
    def __get__(
        self: "FunctionWrapper[Concatenate[T, P2]]",
        instance: T,
    ) -> None:
        raise NotImplementedError

def wrapper(fn: Callable[P, Any]) -> FunctionWrapper[P]:
    raise NotImplementedError

class Example:
    @wrapper
    def __call__(self) -> None:
        pass
```

An invalid descriptor receiver must not discard the inferred `ParamSpec` for its bound callable.
Even though `Concatenate` makes the receiver positional-only, the remaining parameters still retain
their precise types.

```py
class Decorator(Generic[P]):
    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> None: ...
    def __get__(self: "Decorator[Concatenate[Any, P2]]", instance: Any, owner: Any) -> "Decorator[P2]":
        raise NotImplementedError

def decorate(fn: Callable[P, Any]) -> Decorator[P]:
    raise NotImplementedError

class Decorated:
    @decorate
    def method(self, value: str) -> None: ...

# error: [invalid-attribute-access]
bound = Decorated().method
reveal_type(bound)  # revealed: Decorator[(value: str)]
bound(1)  # error: [invalid-argument-type]
```

[descriptors]: https://docs.python.org/3/howto/descriptor.html
[precedence chain]: https://github.com/python/cpython/blob/3.13/Objects/typeobject.c#L5393-L5481
[simple example]: https://docs.python.org/3/howto/descriptor.html#simple-example-a-descriptor-that-returns-a-constant
