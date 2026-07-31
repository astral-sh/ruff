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

### Enum complements preserve descriptor errors

Attribute lookup expands a narrowed enum complement into its remaining literals. A descriptor
failure shared by every remaining literal is still definite.

`enum_descriptor.pyi`:

```pyi
from enum import Enum

class Descriptor:
    def __get__(self) -> int: ...

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3

    descriptor: Descriptor
```

```py
from enum_descriptor import Color

def narrowed(color: Color) -> None:
    if color is Color.RED:
        return

    # error: [invalid-attribute-access]
    color.descriptor
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

This test makes sure that we call `__get__` with the right argument types for various scenarios:

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

# Invalid calls emit a diagnostic. However, we use the return type of `__get__`
# as the inferred type anyway:
# the way to specify that the descriptor object itself is returned when the
# attribute is accessed on the instance or the class is by overloading `__get__`.
#
# Using the return type of `__get__` even for `__get__` calls that have invalid
# arguments passed to them avoids false positives in situations where there are
# `__get__` calls that we don't sufficiently understand.
# error: [invalid-attribute-access] "Invalid access to descriptor attribute `class_object_access` on type `C`"
reveal_type(C().class_object_access)  # revealed: int
# error: [invalid-attribute-access] "Invalid access to descriptor attribute `instance_access` on type `<class 'C'>`"
reveal_type(C.instance_access)  # revealed: str
```

### Descriptors with incorrect `__get__` signature

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

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `descriptor` on type `C`"
reveal_type(C().descriptor)  # revealed: int
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

### Union-like descriptor types use their concrete branches

Descriptor calls through a type alias or constrained type variable use each concrete descriptor type
as the synthetic `self` argument. A valid descriptor should not fail merely because its union-like
wrapper is not assignable to one of its elements.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypeVar

class IntDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> int:
        return 1

class StrDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> str:
        return ""

type DescriptorAlias = IntDescriptor | StrDescriptor

def aliased_descriptor() -> DescriptorAlias:
    raise NotImplementedError

class AliasedOwner:
    value = aliased_descriptor()

reveal_type(AliasedOwner().value)  # revealed: int | str

T = TypeVar("T", IntDescriptor, StrDescriptor)

def access_constrained_descriptor(descriptor: T) -> None:
    class ConstrainedOwner:
        value = descriptor

    reveal_type(ConstrainedOwner().value)  # revealed: int | str

BoundT = TypeVar("BoundT", bound=IntDescriptor | StrDescriptor)

def access_upper_bounded_descriptor(descriptor: BoundT) -> None:
    class UpperBoundedOwner:
        value = descriptor

    reveal_type(UpperBoundedOwner().value)  # revealed: int | str
```

Recursive aliases are expanded behind a cycle-aware query. A recursive branch cannot establish a
definite descriptor failure, and inspecting an ordinary recursive attribute must terminate.

```py
type Recursive = int | Recursive

class RecursiveOwner:
    value: Recursive = 1

RecursiveOwner().value
```

### Constrained receiver types preserve branch correlation

When a constrained type variable is the descriptor receiver, each class constraint is paired with
the descriptor from that same class. The synthetic `instance` argument should therefore use the
concrete class for each branch instead of the unexpanded type variable.

```py
from typing import TypeVar

class IntDescriptor:
    def __get__(self, instance: "IntOwner", owner: type["IntOwner"]) -> int:
        return 1

class IntOwner:
    value = IntDescriptor()

class StrDescriptor:
    def __get__(self, instance: "StrOwner", owner: type["StrOwner"]) -> str:
        return ""

class StrOwner:
    value = StrDescriptor()

T = TypeVar("T", IntOwner, StrOwner)

def access_constrained_owner(owner: T) -> None:
    reveal_type(owner.value)  # revealed: int | str
```

Correlation does not suppress a failure that is present for every constraint.

```py
class BrokenIntDescriptor:
    def __get__(self) -> int:
        return 1

class BrokenIntOwner:
    value = BrokenIntDescriptor()

class BrokenStrDescriptor:
    def __get__(self) -> str:
        return ""

class BrokenStrOwner:
    value = BrokenStrDescriptor()

BrokenT = TypeVar("BrokenT", BrokenIntOwner, BrokenStrOwner)

def access_broken_constrained_owner(owner: BrokenT) -> None:
    # error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `BrokenT@access_broken_constrained_owner`"
    owner.value
```

### Constrained class-object receivers preserve branch correlation

The same correlation applies when the receiver is a class object. Each concrete class constraint is
passed as the synthetic `owner` argument to the descriptor selected from that class.

```py
from typing import TypeVar

class IntDescriptor:
    def __get__(self, instance: None, owner: type["IntOwner"]) -> int:
        return 1

class IntOwner:
    value = IntDescriptor()

class StrDescriptor:
    def __get__(self, instance: None, owner: type["StrOwner"]) -> str:
        return ""

class StrOwner:
    value = StrDescriptor()

T = TypeVar("T", IntOwner, StrOwner)

def access_constrained_class(cls: type[T]) -> None:
    reveal_type(cls.value)  # revealed: int | str
```

Correlation still reports a failure when every concrete class constraint selects a malformed
descriptor.

```py
class BrokenDescriptor:
    def __get__(self) -> bytes:
        return b""

class BrokenIntOwner:
    value = BrokenDescriptor()

class BrokenStrOwner:
    value = BrokenDescriptor()

BrokenT = TypeVar("BrokenT", BrokenIntOwner, BrokenStrOwner)

def access_broken_constrained_class(cls: type[BrokenT]) -> None:
    # error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `type[BrokenT@access_broken_constrained_class]`"
    cls.value
```

### Possible descriptor failures are not reported

An attribute access is invalid only if every possible value invokes a malformed descriptor. A valid
descriptor or normal value makes the failure merely possible, so ty preserves the inferred return
types without emitting `invalid-attribute-access`.

```py
class BrokenDescriptor:
    def __get__(self) -> bytes:
        return b""

class ValidDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> str:
        return ""

def descriptor_or_descriptor() -> BrokenDescriptor | ValidDescriptor:
    raise NotImplementedError

def descriptor_or_value() -> BrokenDescriptor | int:
    raise NotImplementedError

class C:
    descriptor = descriptor_or_descriptor()
    value = descriptor_or_value()

reveal_type(C().descriptor)  # revealed: bytes | str
reveal_type(C().value)  # revealed: bytes | int
```

If every possible value is a malformed descriptor, the failure is definite and is reported.

```py
class BrokenIntDescriptor:
    def __get__(self) -> int:
        return 1

class BrokenStrDescriptor:
    def __get__(self) -> str:
        return ""

def broken_descriptor() -> BrokenIntDescriptor | BrokenStrDescriptor:
    raise NotImplementedError

class C:
    descriptor = broken_descriptor()

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `descriptor` on type `C`"
reveal_type(C().descriptor)  # revealed: int | str
```

### Mixed metaclass descriptor kinds make fallback failures possible

A metaclass member that can be a data descriptor takes precedence over a class attribute on that
branch. If the data descriptor is valid, a malformed class descriptor is therefore only possibly
invoked. This test intentionally does not assert the inferred type for the mixed-precedence lookup.

```py
class ValidDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> str:
        return ""

    def __set__(self, instance: object, value: object) -> None:
        pass

def descriptor_or_value() -> ValidDataDescriptor | int:
    raise NotImplementedError

class BrokenDescriptor:
    def __get__(self) -> bytes:
        return b""

class Meta(type):
    value = descriptor_or_value()

class C(metaclass=Meta):
    value = BrokenDescriptor()

C.value
```

### Conditional descriptor kinds make metaclass failures possible

A conditionally defined `__set__` method makes the metaclass member a data descriptor on only some
paths. On the non-data path, the class attribute takes precedence, so the malformed `__get__` call
is not definite.

```py
def conditional_descriptor_kind(flag: bool) -> None:
    class Descriptor:
        def __get__(self) -> int:
            return 1

        if flag:
            def __set__(self, instance: object, value: object) -> None:
                pass

    class Meta(type):
        value = Descriptor()

    class C(metaclass=Meta):
        value = 1

    C.value
```

### TypeVar data-descriptor kinds retain metaclass precedence

When every constraint or upper-bound alternative is a data descriptor, the TypeVar-valued metaclass
member always takes precedence over a class attribute. A malformed `__get__` call is therefore
definite on every branch.

```py
from typing import TypeVar

class BrokenIntDataDescriptor:
    def __get__(self) -> int:
        return 1

    def __set__(self, instance: object, value: object) -> None:
        pass

class BrokenStrDataDescriptor:
    def __get__(self) -> str:
        return ""

    def __set__(self, instance: object, value: object) -> None:
        pass

ConstrainedDataT = TypeVar(
    "ConstrainedDataT",
    BrokenIntDataDescriptor,
    BrokenStrDataDescriptor,
)
BoundDataT = TypeVar(
    "BoundDataT",
    bound=BrokenIntDataDescriptor | BrokenStrDataDescriptor,
)

def constrained_data_descriptor(descriptor: ConstrainedDataT) -> None:
    class Meta(type):
        value = descriptor

    class C(metaclass=Meta):
        value = 1

    # error: [invalid-attribute-access]
    C.value

def upper_bounded_data_descriptor(descriptor: BoundDataT) -> None:
    class Meta(type):
        value = descriptor

    class C(metaclass=Meta):
        value = 1

    # error: [invalid-attribute-access]
    C.value
```

### Possibly undefined `super` members do not fail definitely

When a malformed descriptor is only possibly defined on a base class, the absent path raises an
attribute error without invoking `__get__`. The descriptor failure is therefore not definite,
although the ordinary possibly-missing diagnostic still applies.

```py
def _(flag: bool):
    class BrokenDescriptor:
        def __get__(self) -> int:
            return 1

    class Base:
        if flag:
            value = BrokenDescriptor()

    class Derived(Base):
        def read(self) -> None:
            # error: [possibly-missing-attribute] "Attribute `value` may be missing on object of type `<super: <class 'Derived'>, Self@read>`"
            super().value

    class DefiniteBase:
        value = BrokenDescriptor()

    class DefiniteDerived(DefiniteBase):
        def read_definite(self) -> None:
            # error: [invalid-attribute-access] "Invalid access to descriptor attribute `value` on type `<super: <class 'DefiniteDerived'>, Self@read_definite>`"
            super().value
```

### TypeVar `super` receivers preserve branch correlation

Each constrained or union-bounded `super` branch validates the implicit descriptor call with its
concrete owner. A valid alternative makes the overall failure only possible, while a constrained
owner whose every branch is invalid still produces a diagnostic. Equivalent ordinary `super` types
retain distinct validation branches, so reversing constraint order does not change the result.

```py
from __future__ import annotations

from enum import Enum
from typing import Literal, TypeVar

class Descriptor:
    def __get__(self, instance: "Good", owner: type | None = None) -> int:
        return 1

class Base:
    value = Descriptor()

class Pivot(Base): ...
class Good(Pivot): ...
class Bad(Pivot): ...
class AlsoBad(Pivot): ...

T = TypeVar("T", Good, Bad)

def possibly_valid(owner: T) -> None:
    super(Pivot, owner).value

BrokenT = TypeVar("BrokenT", Bad, AlsoBad)

def always_invalid(owner: BrokenT) -> None:
    # error: [invalid-attribute-access]
    super(Pivot, owner).value

class LiteralDescriptor:
    def __get__(
        self,
        instance: Literal[Member.A, Member.B],
        owner: type | None = None,
    ) -> int:
        return 1

class LiteralBase:
    value = LiteralDescriptor()

class LiteralPivot(LiteralBase): ...

class Member(LiteralPivot, Enum):
    A = 1
    B = 2
    C = 3

LiteralT = TypeVar("LiteralT", Literal[Member.A], Literal[Member.B])

def literal_constraints(owner: LiteralT) -> None:
    super(LiteralPivot, owner).value

class PartialLiteralDescriptor:
    def __get__(
        self,
        instance: Literal[PartialMember.B],
        owner: type | None = None,
    ) -> int:
        return 1

class PartialLiteralBase:
    value = PartialLiteralDescriptor()

class PartialLiteralPivot(PartialLiteralBase): ...

class PartialMember(PartialLiteralPivot, Enum):
    A = 1
    B = 2
    C = 3

OrderedLiteralT = TypeVar(
    "OrderedLiteralT",
    Literal[PartialMember.A],
    Literal[PartialMember.B],
)
ReversedLiteralT = TypeVar(
    "ReversedLiteralT",
    Literal[PartialMember.B],
    Literal[PartialMember.A],
)
BoundLiteralT = TypeVar(
    "BoundLiteralT",
    bound=Literal[PartialMember.A, PartialMember.B],
)

def ordered_literal_constraints(owner: OrderedLiteralT) -> None:
    super(PartialLiteralPivot, owner).value

def reversed_literal_constraints(owner: ReversedLiteralT) -> None:
    super(PartialLiteralPivot, owner).value

def union_upper_bound(owner: BoundLiteralT) -> None:
    super(PartialLiteralPivot, owner).value
```

### Possible `__get__` callable failures are not reported

Conditionally defined methods produce a union of callable types. A valid callable alternative makes
the implicit descriptor call only possibly invalid, even if another alternative has an incompatible
signature.

```py
def _(flag: bool):
    class Descriptor:
        if flag:
            def __get__(self, instance: object, owner: type | None = None) -> int:
                return 1

        else:
            def __get__(self) -> str:
                return ""

    class C:
        descriptor = Descriptor()

    reveal_type(C().descriptor)  # revealed: int | str
```

### A successful `__getattr__` fallback makes descriptor failure possible

When an invalid descriptor is only possibly defined, the path on which the attribute is absent can
succeed through `__getattr__`. The descriptor is therefore not definitely selected.

```py
def _(flag: bool):
    class Descriptor:
        def __get__(self) -> int:
            return 1

    class C:
        if flag:
            descriptor = Descriptor()

        def __getattr__(self, name: str) -> str:
            return name

    reveal_type(C().descriptor)  # revealed: int | str
```

### A custom `__getattribute__` intercepts descriptor access

Python calls a custom `__getattribute__` before applying the normal descriptor lookup algorithm. A
successful override therefore makes invocation of a malformed descriptor non-definite for both
instance and class-object access.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class C:
    descriptor = Descriptor()

    def __getattribute__(self, name: str) -> str:
        return name

# The diagnostic certainty check does not replace the ordinary inferred member type.
reveal_type(C().descriptor)  # revealed: int

class Meta(type):
    descriptor = Descriptor()

    def __getattribute__(self, name: str) -> str:
        return name

class D(metaclass=Meta): ...

reveal_type(D.descriptor)  # revealed: int

def conditional_getattribute(flag: bool) -> None:
    class Conditional:
        descriptor = Descriptor()

        if flag:
            def __getattribute__(self, name: str) -> str:
                return name

    Conditional().descriptor
```

A custom `__getattribute__` also intercepts normal lookup when calling it fails. The access may
still be invalid, but Python never invokes the descriptor, so ty should not attribute that failure
to the descriptor's `__get__` method.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class InvalidAccessor:
    descriptor = Descriptor()
    __getattribute__ = None

InvalidAccessor().descriptor

class InvalidMeta(type):
    descriptor = Descriptor()

    # error: [invalid-method-override] "Invalid override of method `__getattribute__`: Definition is incompatible with `object.__getattribute__`"
    def __getattribute__(self) -> str:
        return ""

class C(metaclass=InvalidMeta): ...

C.descriptor
```

### A definitely assigned instance attribute shadows a broken non-data descriptor

An instance attribute takes precedence over a non-data descriptor. Once the instance attribute is
definitely assigned, accessing it does not call the descriptor's invalid `__get__` method.

```py
from typing import Literal

class Descriptor:
    def __get__(self) -> str:
        return ""

class C:
    descriptor = Descriptor()

    def replace_descriptor(self) -> None:
        self.descriptor: int = 1
        reveal_type(self.descriptor)  # revealed: Literal[1]
```

### A possibly reaching instance assignment makes descriptor failure possible

An instance assignment that reaches a read on only some control-flow paths can still shadow a
non-data descriptor. The descriptor call is not definite because the assigned path reads the
instance dictionary instead.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

class C:
    descriptor = Descriptor()

    def replace_descriptor(self, flag: bool) -> None:
        if flag:
            self.descriptor = Descriptor()
        self.descriptor
```

### Inferred instance attributes are possible shadows

The class-wide instance-member summary does not distinguish an assignment performed by `__init__`
from one in an unrelated method. Because either could represent a successful instance-dictionary
lookup, both suppress the definite descriptor diagnostic. The unrelated-method and deletion cases
document accepted conservative false negatives; modeling their reachability requires object-level
dataflow.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

class C:
    descriptor = Descriptor()

    def __init__(self) -> None:
        self.descriptor = Descriptor()

C().descriptor

class D:
    descriptor = Descriptor()

    def assign_descriptor(self) -> None:
        self.descriptor: int = 1

D().descriptor

class E:
    descriptor = Descriptor()

    def read_after_delete(self) -> None:
        self.descriptor = Descriptor()
        del self.descriptor
        self.descriptor
```

### Reaching assignments conservatively suppress descriptor diagnostics

Ty does not use descriptor kind as proof that a malformed `__get__` remains selected after an
assignment. Even a statically known data descriptor is not diagnosed after a reaching assignment.
This conservative boundary also covers conditional setters and mixed descriptor kinds without
requiring a separate descriptor-certainty model.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

    def __set__(self, instance: object, value: int) -> None:
        pass

class C:
    descriptor = Descriptor()

    def assign_descriptor(self) -> None:
        self.descriptor = 1
        reveal_type(self.descriptor)  # revealed: str
```

A conditional `__set__` makes the descriptor data-like on only some runtime paths. The reaching
assignment suppresses the read diagnostic on both paths.

```py
def conditional_setter(flag: bool) -> None:
    class Descriptor:
        def __get__(self) -> str:
            return ""

        if flag:
            def __set__(self, instance: object, value: int) -> None:
                pass

    class C:
        descriptor = Descriptor()

    c = C()
    c.descriptor = 1
    c.descriptor
```

### Augmented assignment reads before writing

An augmented assignment first reads the descriptor and then writes the operation's result. A data
descriptor with a malformed `__get__` therefore produces an access diagnostic even when its
`__set__` method accepts the result. Deletion invokes `__delete__` without invoking `__get__`.

```py
class Descriptor:
    def __get__(self) -> int:
        return 1

    def __set__(self, instance: object, value: int) -> None:
        pass

    def __delete__(self, instance: object) -> None:
        pass

class C:
    descriptor = Descriptor()

c = C()
# error: [invalid-attribute-access] "Invalid access to descriptor attribute `descriptor` on type `C`"
c.descriptor += 1
del c.descriptor
```

### Reaching assignments suppress mixed-descriptor diagnostics

If an attribute could contain either a data or non-data descriptor, an assignment can shadow the
non-data branch. Ty suppresses the descriptor diagnostic after the reaching assignment regardless of
the aggregate descriptor kind.

```py
from typing import Union

class BrokenNonData:
    def __get__(self) -> str:
        return ""

class BrokenData:
    def __get__(self) -> bytes:
        return b""

    def __set__(self, instance: object, value: int) -> None:
        pass

def make_descriptor() -> Union[BrokenNonData, BrokenData]:
    return BrokenData()

class C:
    descriptor = make_descriptor()

c = C()
# error: [invalid-assignment]
c.descriptor = 1
c.descriptor
```

### Class-namespace mutations are not propagated to instance lookup

Ty tracks assignments to syntactic places such as `C.descriptor`, but it does not propagate that
mutation through semantic class identity to `C().descriptor`, aliases, or subclasses. The static
class attribute type includes a normal value in these examples, so the malformed descriptor call is
not definite and no `invalid-attribute-access` diagnostic is emitted.

```py
class BrokenDescriptor:
    def __get__(self) -> str:
        return ""

class C:
    descriptor: BrokenDescriptor | int = BrokenDescriptor()

class Subclass(C): ...

C.descriptor = 1
reveal_type(C.descriptor)  # revealed: Literal[1]
reveal_type(C().descriptor)  # revealed: str | int
reveal_type(Subclass().descriptor)  # revealed: str | int

Alias = C
Alias.descriptor = 1
reveal_type(C().descriptor)  # revealed: str | int
```

The same limitation means that assigning a malformed descriptor through a class object is not enough
to make a later instance diagnostic definite.

```py
class BrokenDescriptor:
    def __get__(self) -> str:
        return ""

class C:
    descriptor: BrokenDescriptor | int = 1

C.descriptor = BrokenDescriptor()
C.descriptor
reveal_type(C().descriptor)  # revealed: str | int
```

The same conservative boundary applies when a metaclass data descriptor intercepts a class-object
assignment. Ty does not preserve the lookup stage through same-place assignment flow, so it does not
diagnose the subsequent read even though runtime still invokes the malformed descriptor.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

    def __set__(self, instance: object, value: int) -> None:
        pass

class Meta(type):
    descriptor = Descriptor()

class C(metaclass=Meta): ...

C.descriptor = 1
C.descriptor
```

### Undefined intersection elements do not suppress descriptor failures

An intersection element that does not define the attribute does not contribute an alternative
member. The member supplied by another element remains definitely selected.

```py
from ty_extensions import Intersection

class Descriptor:
    def __get__(self) -> int:
        return 1

class DefinesDescriptor:
    descriptor = Descriptor()

class Marker: ...

def access_intersection(obj: Intersection[DefinesDescriptor, Marker]) -> None:
    # error: [invalid-attribute-access] "Invalid access to descriptor attribute `descriptor` on type `DefinesDescriptor & Marker`"
    obj.descriptor
```

### Non-descriptor attribute intersections are refinements

An attribute intersection represents one runtime value satisfying every positive element. A positive
element without `__get__` refines the descriptor value; it is not an alternative that can avoid the
descriptor call.

```py
from ty_extensions import Intersection

class Descriptor:
    def __get__(self) -> int:
        return 1

class Marker: ...

def make_descriptor() -> Intersection[Descriptor, Marker]:
    raise NotImplementedError

class C:
    descriptor = make_descriptor()

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `descriptor` on type `C`"
C().descriptor
```

### Data-descriptor intersections retain metaclass precedence

An intersection describes one runtime value. If any positive element is definitely a data
descriptor, a class attribute cannot shadow the intersection-valued metaclass member. A non-data
descriptor intersection remains shadowed as usual.

```py
from ty_extensions import Intersection

class BrokenDataDescriptor:
    def __get__(self) -> int:
        return 1

    def __set__(self, instance: object, value: object) -> None:
        pass

class BrokenNonDataDescriptor:
    def __get__(self) -> str:
        return ""

class Marker: ...

def data_descriptor() -> Intersection[BrokenDataDescriptor, Marker]:
    raise NotImplementedError

def non_data_descriptor() -> Intersection[BrokenNonDataDescriptor, Marker]:
    raise NotImplementedError

class Meta(type):
    data = data_descriptor()
    non_data = non_data_descriptor()

class C(metaclass=Meta):
    data = 1
    non_data = 1

# error: [invalid-attribute-access] "Invalid access to descriptor attribute `data` on type `<class 'C'>`"
C.data
C.non_data
```

### A shadowed metaclass descriptor with an incorrect `__get__` signature

A class attribute takes precedence over a non-data descriptor of the same name on the metaclass, so
the descriptor's invalid `__get__` signature is irrelevant to this access.

```py
class Descriptor:
    def __get__(self) -> str:
        return ""

class Meta(type):
    attribute = Descriptor()

class C(metaclass=Meta):
    attribute = 1

reveal_type(C.attribute)  # revealed: int
```

### "Descriptors" with non-callable `__get__` attributes

If `__get__` is not callable at all, the interpreter will still attempt to call the method at
runtime, and this will raise an exception. As such, even for `__get__ = None`, we still "attempt to
call `__get__`" on the descriptor object (leading us to infer `Unknown`):

```py
class BrokenDescriptor:
    __get__: None = None

class Foo:
    desc: BrokenDescriptor = BrokenDescriptor()

# This raises `TypeError` at runtime due to the implicit call to `__get__`.
# error: [invalid-attribute-access] "Invalid access to descriptor attribute `desc` on type `Foo`"
reveal_type(Foo().desc)  # revealed: Unknown
```

### A classmethod `__get__` slot is not callable

Implicit descriptor invocation reads the raw `__get__` value from the descriptor class without
binding it. A `classmethod` object is not callable, even when its underlying function annotations
accept all three synthesized arguments.

```py
class Descriptor:
    @classmethod
    def __get__(cls: object, instance: object, owner: object) -> int:
        return 1

class C:
    value = Descriptor()

# error: [invalid-attribute-access]
reveal_type(C().value)  # revealed: int
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

[descriptors]: https://docs.python.org/3/howto/descriptor.html
[precedence chain]: https://github.com/python/cpython/blob/3.13/Objects/typeobject.c#L5393-L5481
[simple example]: https://docs.python.org/3/howto/descriptor.html#simple-example-a-descriptor-that-returns-a-constant
