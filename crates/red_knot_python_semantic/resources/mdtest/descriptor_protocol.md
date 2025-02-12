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
    ten = Ten()

c = C()

reveal_type(c.ten)  # revealed: Literal[10]

reveal_type(C.ten)  # revealed: Literal[10]

# These are fine:
c.ten = 10
C.ten = 10

# TODO: This should be an error
c.ten = 11

# TODO: This is not the correct error message
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
    flexible_int = FlexibleInt()

c = C()

reveal_type(c.flexible_int)  # revealed: int | None

c.flexible_int = 42  # okay
c.flexible_int = "42"  # also okay!

reveal_type(c.flexible_int)  # revealed: int | None

# TODO: should be an error
c.flexible_int = None  # not okay

reveal_type(c.flexible_int)  # revealed: int | None
```

## Data and non-data descriptors

Descriptors that define `__set__` or `__delete__` are called *data descriptors* (e.g. properties),
while those that only define `__get__` are called non-data descriptors (e.g. functions,
`classmethod` or `staticmethod`).

The precedence chain for attribute access is (1) data descriptors, (2) instance attributes, and (3)
non-data descriptors.

```py
from typing import Literal

class DataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["data"]:
        return "data"

    def __set__(self, instance: object, value) -> None:
        pass

class NonDataDescriptor:
    def __get__(self, instance: object, owner: type | None = None) -> Literal["non-data"]:
        return "non-data"

class C:
    data_descriptor = DataDescriptor()
    non_data_descriptor = NonDataDescriptor()

    def f(self):
        # This explains why data descriptors come first in the precendence chain. If
        # instance attributes would take priority, we would override the descriptor
        # here. Instead, this calls `DataDescriptor.__set__`, i.e. it does not affect
        # the type of the `data_descriptor` attribute.
        self.data_descriptor = 1

        # However, for non-data descriptors, instance attributes do take precedence.
        # So it is possible to override them.
        self.non_data_descriptor = 1

c = C()

reveal_type(c.data_descriptor)  # revealed: Literal["data"]

# TODO: This should ideally be `Literal["non-data", 1]`.
#
#     - Mypy does not support this either and only shows `Literal['non-data']`
#     - Pyright shows `int | Literal['non-data']` here, but also wrongly shows the
#       same for all other three cases.
#
reveal_type(c.non_data_descriptor)  # revealed: Literal["non-data"]

reveal_type(C.data_descriptor)  # revealed: Literal["data"]

reveal_type(C.non_data_descriptor)  # revealed: Literal["non-data"]
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
        self.ten = Ten()

# TODO: Should be Unknown | Ten
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
    d = Descriptor()

# TODO: should be `Literal["called on class object"]
reveal_type(C.d)  # revealed: LiteralString

# TODO: should be `Literal["called on instance"]
reveal_type(C().d)  # revealed: LiteralString
```

[descriptors]: https://docs.python.org/3/howto/descriptor.html
[simple example]: https://docs.python.org/3/howto/descriptor.html#simple-example-a-descriptor-that-returns-a-constant
