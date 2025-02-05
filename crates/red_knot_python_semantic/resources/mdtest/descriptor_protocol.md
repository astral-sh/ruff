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
