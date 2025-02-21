# Descriptor protocol

[Descriptors] let objects customize attribute lookup, storage, and deletion.

A descriptor is an attribute value that has one of the methods in the descriptor protocol. Those
methods are `__get__()`, `__set__()`, and `__delete__()`. If any of those methods are defined for an
attribute, it is said to be a descriptor.

## Basic example

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

# These are fine:
# TODO: This should not be an error
c.ten = 10  # error: [invalid-assignment]
C.ten = 10

# TODO: This should be an error (as the wrong type is being implicitly passed to `Ten.__set__`),
# but the error message is misleading.
# error: [invalid-assignment] "Object of type `Literal[11]` is not assignable to attribute `ten` of type `Ten`"
c.ten = 11

# TODO: same as above
# error: [invalid-assignment] "Object of type `Literal[11]` is not assignable to attribute `ten` of type `Literal[10]`"
C.ten = 11
```

## Different types for `__get__` and `__set__`

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

# TODO: These should not be errors
# error: [invalid-assignment]
c.flexible_int = 42  # okay
# error: [invalid-assignment]
c.flexible_int = "42"  # also okay!

reveal_type(c.flexible_int)  # revealed: int | None

# TODO: This should be an error, but the message needs to be improved.
# error: [invalid-assignment] "Object of type `None` is not assignable to attribute `flexible_int` of type `FlexibleInt`"
c.flexible_int = None  # not okay

reveal_type(c.flexible_int)  # revealed: int | None
```

## Data and non-data descriptors

Descriptors that define `__set__` or `__delete__` are called *data descriptors*. An example\
of a data descriptor is a `property` with a setter and/or a deleter.\
Descriptors that only define `__get__`, meanwhile, are called *non-data descriptors*. Examples
include\
functions, `classmethod` or `staticmethod`).

The precedence chain for attribute access is (1) data descriptors, (2) instance attributes, and (3)
non-data descriptors.

```py
from typing import Literal

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: int, value) -> None:
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

# TODO: This should ideally be `Unknown | Literal["data"]`.
#
#     - Pyright also wrongly shows `int | Literal['data']` here
#     - Mypy shows Literal["data"] here, but also shows Literal["non-data"] below.
#
reveal_type(c.data_descriptor)  # revealed: Unknown | Literal["data", 1]

reveal_type(c.non_data_descriptor)  # revealed: Unknown | Literal["non-data", 1]

reveal_type(C.data_descriptor)  # revealed: Unknown | Literal["data"]

reveal_type(C.non_data_descriptor)  # revealed: Unknown | Literal["non-data"]

# It is possible to override data descriptors via class objects. The following
# assignment does not call `DataDescriptor.__set__`. For this reason, we infer
# `Unknown | …` for all (descriptor) attributes.
C.data_descriptor = "something else"  # This is okay
```

## Built-in `property` descriptor

The built-in `property` decorator creates a descriptor. The names for attribute reads/writes are
determined by the return type of the `name` method and the parameter type of the setter,
respectively.

```py
class C:
    _name: str | None = None

    @property
    def name(self) -> str:
        return self._name or "Unset"
    # TODO: No diagnostic should be emitted here
    # error: [unresolved-attribute] "Type `Literal[name]` has no attribute `setter`"
    @name.setter
    def name(self, value: str | None) -> None:
        self._value = value

c = C()

reveal_type(c._name)  # revealed: str | None

# Should be `str`
reveal_type(c.name)  # revealed: @Todo(decorated method)

# Should be `builtins.property`
reveal_type(C.name)  # revealed: Literal[name]

# This is fine:
c.name = "new"

c.name = None

# TODO: this should be an error
c.name = 42
```

## Built-in `classmethod` descriptor

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

# TODO: should be `C`
reveal_type(c1)  # revealed: @Todo(return type)

# TODO: should be `str`
reveal_type(C.get_name())  # revealed: @Todo(return type)

# TODO: should be `str`
reveal_type(C("42").get_name())  # revealed: @Todo(decorated method)
```

## Descriptors only work when used as class variables

From the descriptor guide:

> Descriptors only work when used as class variables. When put in instances, they have no effect.

```py
from typing import Literal

class Ten:
    def __get__(self, instance: object, owner: type | None = None) -> Literal[10]:
        return 10

class C:
    def __init__(self):
        self.ten: Ten = Ten()

# TODO: Should be Ten
reveal_type(C().ten)  # revealed: Literal[10]
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

## Undeclared descriptor arguments

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

## `__get__` is called with correct arguments

```py
from __future__ import annotations

class TailoredForClassObjectAccess:
    def __get__(self, instance: None, owner: type[C]) -> int:
        return 1

class TailoredForInstanceAccess:
    def __get__(self, instance: C, owner: type[C] | None = None) -> str:
        return "a"

class C:
    class_object_access: TailoredForClassObjectAccess = TailoredForClassObjectAccess()
    instance_access: TailoredForInstanceAccess = TailoredForInstanceAccess()

reveal_type(C.class_object_access)  # revealed: int
reveal_type(C().instance_access)  # revealed: str

# TODO: These should emit a diagnostic
reveal_type(C().class_object_access)  # revealed: TailoredForClassObjectAccess
reveal_type(C.instance_access)  # revealed: TailoredForInstanceAccess
```

## Descriptors with incorrect `__get__` signature

```py
class Descriptor:
    # `__get__` method with missing parameters:
    def __get__(self) -> int:
        return 1

class C:
    descriptor: Descriptor = Descriptor()

# TODO: This should be an error
reveal_type(C.descriptor)  # revealed: Descriptor
```

## Possibly-unbound `__get__` method

```py
def _(flag: bool):
    class MaybeDescriptor:
        if flag:
            def __get__(self, instance: object, owner: type | None = None) -> int:
                return 1

    class C:
        descriptor: MaybeDescriptor = MaybeDescriptor()

    # TODO: This should be `MaybeDescriptor | int`
    reveal_type(C.descriptor)  # revealed: int
```

## Dunder methods

Dunder methods are looked up on the meta type, but we still need to invoke the descriptor protocol:

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

## Functions as descriptors

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
reveal_type(wrapper_descriptor.__qualname__)  # revealed: @Todo(@property)
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
# error: [missing-argument] "No arguments provided for required parameters `self`, `instance`"
wrapper_descriptor()

# Calling it without the `instance` argument is an also an
# error: [missing-argument] "No argument provided for required parameter `instance`"
wrapper_descriptor(f)

# Calling it without the `owner` argument if `instance` is not `None` is an
# error: [missing-argument] "No argument provided for required parameter `owner`"
wrapper_descriptor(f, None)

# But calling it with an instance is fine (in this case, the `owner` argument is optional):
wrapper_descriptor(f, C())

# Calling it with something that is not a `FunctionType` as the first argument is an
# error: [invalid-argument-type] "Object of type `Literal[1]` cannot be assigned to parameter 1 (`self`); expected type `FunctionType`"
wrapper_descriptor(1, None, type(f))

# Calling it with something that is not a `type` as the `owner` argument is an
# error: [invalid-argument-type] "Object of type `Literal[f]` cannot be assigned to parameter 3 (`owner`); expected type `type`"
wrapper_descriptor(f, None, f)

# Calling it with too many positional arguments is an
# error: [too-many-positional-arguments] "Too many positional arguments: expected 3, got 4"
wrapper_descriptor(f, None, type(f), "one too many")
```

[descriptors]: https://docs.python.org/3/howto/descriptor.html
[simple example]: https://docs.python.org/3/howto/descriptor.html#simple-example-a-descriptor-that-returns-a-constant
