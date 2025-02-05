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

# TODO: this should be `Literal[10]`
reveal_type(c.ten)  # revealed: Unknown | Ten

# TODO: This should `Literal[10]`
reveal_type(C.ten)  # revealed: Unknown | Ten

# These are fine:
c.ten = 10
C.ten = 10

# TODO: Both of these should be errors
c.ten = 11
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

# TODO: should be `int | None`
reveal_type(c.flexible_int)  # revealed: Unknown | FlexibleInt

c.flexible_int = 42  # okay
c.flexible_int = "42"  # also okay!

# TODO: should be `int | None`
reveal_type(c.flexible_int)  # revealed: Unknown | FlexibleInt

# TODO: should be an error
c.flexible_int = None  # not okay

# TODO: should be `int | None`
reveal_type(c.flexible_int)  # revealed: Unknown | FlexibleInt
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
reveal_type(c.name)  # revealed: @Todo(bound method)

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
reveal_type(C("42").get_name())  # revealed: @Todo(bound method)
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

reveal_type(C().ten)  # revealed: Unknown | Ten
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
reveal_type(C.d)  # revealed: Unknown | Descriptor

# TODO: should be `Literal["called on instance"]
reveal_type(C().d)  # revealed: Unknown | Descriptor
```

[descriptors]: https://docs.python.org/3/howto/descriptor.html
[simple example]: https://docs.python.org/3/howto/descriptor.html#simple-example-a-descriptor-that-returns-a-constant
