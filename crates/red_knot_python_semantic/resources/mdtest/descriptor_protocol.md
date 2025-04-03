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

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `ten` on type `C` with custom `__set__` method"
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

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `flexible_int` on type `C` with custom `__set__` method"
c.flexible_int = None  # not okay

reveal_type(c.flexible_int)  # revealed: int | None
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
        self.non_data_descriptor = 1

c = C()

reveal_type(c.data_descriptor)  # revealed: Unknown | Literal["data"]

reveal_type(c.non_data_descriptor)  # revealed: Unknown | Literal["non-data", 1]

reveal_type(C.data_descriptor)  # revealed: Unknown | Literal["data"]

reveal_type(C.non_data_descriptor)  # revealed: Unknown | Literal["non-data"]

# It is possible to override data descriptors via class objects. The following
# assignment does not call `DataDescriptor.__set__`. For this reason, we infer
# `Unknown | …` for all (descriptor) attributes.
C.data_descriptor = "something else"  # This is okay
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
            self.attr = "normal"

    reveal_type(C1().attr)  # revealed: Unknown | Literal["data", "normal"]

    # Assigning to the attribute also causes no `possibly-unbound` diagnostic:
    C1().attr = 1
```

We never treat implicit instance attributes as definitely bound, so we fall back to the non-data
descriptor here:

```py
class C2:
    def f(self):
        self.attr = "normal"
    attr = NonDataDescriptor()

reveal_type(C2().attr)  # revealed: Unknown | Literal["non-data", "normal"]

# Assignments always go to the instance attribute in this case
C2().attr = 1
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

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `meta_data_descriptor` on type `Literal[C1]` with custom `__set__` method"
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

When a metaclass data descriptor is possibly unbound, we union the result type of its `__get__`
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
    # error: [possibly-unbound-attribute]
    reveal_type(C5.meta_data_descriptor2)  # revealed: Literal["data"]

    # TODO: We currently emit two diagnostics here, corresponding to the two states of `flag`. The diagnostics are not
    # wrong, but they could be subsumed under a higher-level diagnostic.

    # error: [invalid-assignment] "Invalid assignment to data descriptor attribute `meta_data_descriptor1` on type `Literal[C5]` with custom `__set__` method"
    # error: [invalid-assignment] "Object of type `None` is not assignable to attribute `meta_data_descriptor1` of type `Literal["value on class"]`"
    C5.meta_data_descriptor1 = None

    # error: [possibly-unbound-attribute]
    C5.meta_data_descriptor2 = 1
```

When a class-level attribute is possibly unbound, we union its (descriptor protocol) type with the
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
    # error: [possibly-unbound-attribute]
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

# TODO: should be `Literal["called on class object"]
reveal_type(C.d)  # revealed: LiteralString

# TODO: should be `Literal["called on instance"]
reveal_type(C().d)  # revealed: LiteralString
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

# error: [invalid-assignment] "Invalid assignment to data descriptor attribute `name` on type `C` with custom `__set__` method"
c.name = 42
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

### Functions as descriptors

Functions are descriptors because they implement a `__get__` method. This is crucial in making sure
that method calls work as expected. See [this test suite](./call/methods.md) for more information.
Here, we only demonstrate how `__get__` works on functions:

```py
from inspect import getattr_static

def f(x: object) -> str:
    return "a"

reveal_type(f)  # revealed: Literal[f]
reveal_type(f.__get__)  # revealed: <method-wrapper `__get__` of `f`>
reveal_type(f.__get__(None, type(f)))  # revealed: Literal[f]
reveal_type(f.__get__(None, type(f))(1))  # revealed: str

wrapper_descriptor = getattr_static(f, "__get__")

reveal_type(wrapper_descriptor)  # revealed: <wrapper-descriptor `__get__` of `function` objects>
reveal_type(wrapper_descriptor(f, None, type(f)))  # revealed: Literal[f]

# Attribute access on the method-wrapper `f.__get__` falls back to `MethodWrapperType`:
reveal_type(f.__get__.__hash__)  # revealed: <bound method `__hash__` of `MethodWrapperType`>

# Attribute access on the wrapper-descriptor falls back to `WrapperDescriptorType`:
reveal_type(wrapper_descriptor.__qualname__)  # revealed: str
```

We can also bind the free function `f` to an instance of a class `C`:

```py
class C: ...

bound_method = wrapper_descriptor(f, C(), C)

reveal_type(bound_method)  # revealed: <bound method `f` of `C`>
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

# Calling it without the `owner` argument if `instance` is not `None` is an
# error: [no-matching-overload] "No overload of wrapper descriptor `FunctionType.__get__` matches arguments"
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

# TODO: These should emit a diagnostic
reveal_type(C().class_object_access)  # revealed: TailoredForClassObjectAccess
reveal_type(C.instance_access)  # revealed: TailoredForInstanceAccess
```

### Descriptors with incorrect `__get__` signature

```py
class Descriptor:
    # `__get__` method with missing parameters:
    def __get__(self) -> int:
        return 1

class C:
    descriptor: Descriptor = Descriptor()

# TODO: This should be an error
reveal_type(C.descriptor)  # revealed: Descriptor

# TODO: This should be an error
reveal_type(C().descriptor)  # revealed: Descriptor
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

C.descriptor = "something else"

# This could also be `Literal["something else"]` if we support narrowing of attribute types based on assignments
reveal_type(C.descriptor)  # revealed: Unknown | int
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

    # error: [possibly-unbound-attribute] "Attribute `non_data` on type `Literal[PossiblyUnbound]` is possibly unbound"
    reveal_type(PossiblyUnbound.non_data)  # revealed: int

    # error: [possibly-unbound-attribute] "Attribute `non_data` on type `PossiblyUnbound` is possibly unbound"
    reveal_type(PossiblyUnbound().non_data)  # revealed: int

    # error: [possibly-unbound-attribute] "Attribute `data` on type `Literal[PossiblyUnbound]` is possibly unbound"
    reveal_type(PossiblyUnbound.data)  # revealed: int

    # error: [possibly-unbound-attribute] "Attribute `data` on type `PossiblyUnbound` is possibly unbound"
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

[descriptors]: https://docs.python.org/3/howto/descriptor.html
[precedence chain]: https://github.com/python/cpython/blob/3.13/Objects/typeobject.c#L5393-L5481
[simple example]: https://docs.python.org/3/howto/descriptor.html#simple-example-a-descriptor-that-returns-a-constant
