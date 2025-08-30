# Self

```toml
[environment]
python-version = "3.11"
```

`Self` is treated as if it were a `TypeVar` bound to the class it's being used on.

`typing.Self` is only available in Python 3.11 and later.

## Methods

```py
from typing import Self

class Shape:
    def set_scale(self: Self, scale: float) -> Self:
        reveal_type(self)  # revealed: Self@set_scale
        return self

    def nested_type(self: Self) -> list[Self]:
        return [self]

    def nested_func(self: Self) -> Self:
        def inner() -> Self:
            reveal_type(self)  # revealed: Self@nested_func
            return self
        return inner()

    def nested_func_without_enclosing_binding(self):
        def inner(x: Self):
            # TODO: revealed: Self@nested_func_without_enclosing_binding
            # (The outer method binds an implicit `Self`)
            reveal_type(x)  # revealed: Self@inner
        inner(self)

    def implicit_self(self) -> Self:
        # TODO: first argument in a method should be considered as "typing.Self"
        reveal_type(self)  # revealed: Unknown
        return self

reveal_type(Shape().nested_type())  # revealed: list[Shape]
reveal_type(Shape().nested_func())  # revealed: Shape

class Circle(Shape):
    def set_scale(self: Self, scale: float) -> Self:
        reveal_type(self)  # revealed: Self@set_scale
        return self

class Outer:
    class Inner:
        def foo(self: Self) -> Self:
            reveal_type(self)  # revealed: Self@foo
            return self
```

## typing_extensions

```toml
[environment]
python-version = "3.10"
```

```py
from typing_extensions import Self

class C:
    def method(self: Self) -> Self:
        return self

reveal_type(C().method())  # revealed: C
```

## Class Methods

```py
from typing import Self, TypeVar

class Shape:
    def foo(self: Self) -> Self:
        return self

    @classmethod
    def bar(cls: type[Self]) -> Self:
        # TODO: type[Shape]
        reveal_type(cls)  # revealed: @Todo(unsupported type[X] special form)
        return cls()

class Circle(Shape): ...

reveal_type(Shape().foo())  # revealed: Shape
# TODO: Shape
reveal_type(Shape.bar())  # revealed: Unknown
```

## Attributes

TODO: The use of `Self` to annotate the `next_node` attribute should be
[modeled as a property][self attribute], using `Self` in its parameter and return type.

```py
from typing import Self

class LinkedList:
    value: int
    next_node: Self

    def next(self: Self) -> Self:
        reveal_type(self.value)  # revealed: int
        # TODO: no error
        # error: [invalid-return-type]
        return self.next_node

reveal_type(LinkedList().next())  # revealed: LinkedList
```

## Generic Classes

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

class Container(Generic[T]):
    value: T
    def set_value(self: Self, value: T) -> Self:
        return self

int_container: Container[int] = Container[int]()
reveal_type(int_container)  # revealed: Container[int]
reveal_type(int_container.set_value(1))  # revealed: Container[int]
```

## Protocols

TODO: <https://typing.python.org/en/latest/spec/generics.html#use-in-protocols>

## Annotations

```py
from typing import Self

class Shape:
    def union(self: Self, other: Self | None):
        reveal_type(other)  # revealed: Self@union | None
        return self
```

## Invalid Usage

`Self` cannot be used in the signature of a function or variable.

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

# error: [invalid-type-form]
def x(s: Self): ...

# error: [invalid-type-form]
b: Self

# TODO: "Self" cannot be used in a function with a `self` or `cls` parameter that has a type annotation other than "Self"
class Foo:
    # TODO: rejected Self because self has a different type
    def has_existing_self_annotation(self: T) -> Self:
        return self  # error: [invalid-return-type]

    def return_concrete_type(self) -> Self:
        # TODO: tell user to use "Foo" instead of "Self"
        # error: [invalid-return-type]
        return Foo()

    @staticmethod
    # TODO: reject because of staticmethod
    def make() -> Self:
        # error: [invalid-return-type]
        return Foo()

class Bar(Generic[T]):
    foo: T
    def bar(self) -> T:
        return self.foo

# error: [invalid-type-form]
class Baz(Bar[Self]): ...

class MyMetaclass(type):
    # TODO: rejected
    def __new__(cls) -> Self:
        return super().__new__(cls)
```

## Binding a method fixes `Self`

When a method is bound, any instances of `Self` in its signature are "fixed", since we now know the
specific type of the bound parameter.

```py
from typing import Self

class C:
    def instance_method(self, other: Self) -> Self:
        return self

    @classmethod
    def class_method(cls) -> Self:
        return cls()

# revealed: bound method C.instance_method(other: C) -> C
reveal_type(C().instance_method)
# revealed: bound method <class 'C'>.class_method() -> C
reveal_type(C.class_method)

class D(C): ...

# revealed: bound method D.instance_method(other: D) -> D
reveal_type(D().instance_method)
# revealed: bound method <class 'D'>.class_method() -> D
reveal_type(D.class_method)
```

## Constructor `__new__` return type handling

The type checker respects explicit return type annotations on `__new__` methods.

```py
from typing import Self

# Non-instance return type
class A:
    def __new__(cls) -> "int | A":
        import random

        return 42 if random.random() > 0.5 else object.__new__(cls)

reveal_type(A())  # revealed: int | A

# Self return type
class B:
    def __new__(cls) -> Self:
        return super().__new__(cls)

reveal_type(B())  # revealed: B

# Generic class with explicit return type
from typing import TypeVar, Generic, Tuple

T = TypeVar("T")

class SendChannel(Generic[T]):
    pass

class ReceiveChannel(Generic[T]):
    pass

class ChannelPair(Tuple[SendChannel[T], ReceiveChannel[T]], Generic[T]):
    def __new__(cls, buffer_size: int) -> Tuple[SendChannel[T], ReceiveChannel[T]]:
        # In reality would create and return the tuple
        return (SendChannel[T](), ReceiveChannel[T]())

    def __init__(self, buffer_size: int):
        pass

# Test generic specialization
int_channel = ChannelPair[int](5)
reveal_type(int_channel)  # revealed: tuple[SendChannel[int], ReceiveChannel[int]]

str_channel = ChannelPair[str](10)
reveal_type(str_channel)  # revealed: tuple[SendChannel[str], ReceiveChannel[str]]

# Test unspecialized generic
unspecified_channel = ChannelPair(5)
reveal_type(unspecified_channel)  # revealed: tuple[SendChannel[Unknown], ReceiveChannel[Unknown]]

# Test Any return type (per spec, Any means "not an instance")
from typing import Any

class WithAny:
    def __new__(cls) -> Any:
        return 42

    def __init__(self, required_param: str):
        # This should not be called since __new__ returns Any
        # If it were called, WithAny() would error due to missing required_param
        pass

# This should work (not error) because __init__ shouldn't be called
reveal_type(WithAny())  # revealed: Any

# Test union containing Any (per spec, should also bypass __init__)
class WithUnionAny:
    def __new__(cls) -> "int | Any":
        return 42

    def __init__(self):
        # This should not be called since __new__ returns union with Any
        pass

reveal_type(WithUnionAny())  # revealed: int | Any

# Test non-instance return type (not Any, but also not a subclass)
class ReturnsInt:
    def __new__(cls) -> int:
        return 42

    def __init__(self, required_param: str):
        # This should not be called since __new__ returns int (not an instance)
        pass

# This should work because __init__ shouldn't be called
reveal_type(ReturnsInt())  # revealed: int

# Test NamedTuple - special handling for synthesized __new__ that returns None
from typing import NamedTuple

class Point(NamedTuple):
    x: int
    y: int

# NamedTuple instances are correctly typed
p = Point(1, 2)
reveal_type(p)  # revealed: Point
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int

# Also works with keyword arguments
p2 = Point(x=3, y=4)
reveal_type(p2)  # revealed: Point
```

[self attribute]: https://typing.python.org/en/latest/spec/generics.html#use-in-attribute-annotations
