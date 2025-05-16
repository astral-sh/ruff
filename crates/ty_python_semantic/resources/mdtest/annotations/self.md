# Self

`Self` is treated as if it were a `TypeVar` bound to the class it's being used on.

`typing.Self` is only available in Python 3.11 and later.

## Methods

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Self

class Shape:
    def set_scale(self: Self, scale: float) -> Self:
        reveal_type(self)  # revealed: Self
        return self

    def nested_type(self) -> list[Self]:
        return [self]

    def nested_func(self: Self) -> Self:
        def inner() -> Self:
            reveal_type(self)  # revealed: Self
            return self
        return inner()

    def implicit_self(self) -> Self:
        reveal_type(self)  # revealed: Self
        return self

reveal_type(Shape().nested_type())  # revealed: list[Shape]
reveal_type(Shape().implicit_self())  # revealed: Shape
reveal_type(Shape().nested_func())  # revealed: Shape

class Circle(Shape):
    def set_scale(self: Self, scale: float) -> Self:
        reveal_type(self)  # revealed: Self
        return self

class Outer:
    class Inner:
        def foo(self: Self) -> Self:
            reveal_type(self)  # revealed: Self
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

```toml
[environment]
python-version = "3.11"
```

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

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Self

class LinkedList:
    value: int
    next_node: Self

    def next(self: Self) -> Self:
        reveal_type(self.value)  # revealed: int
        return self.next_node

reveal_type(LinkedList().next())  # revealed: LinkedList
```

## Generic Classes

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

class Container(Generic[T]):
    value: T
    def set_value(self: Self, value: T) -> Self:
        return self

c = Container[int]()
reveal_type(c.set_value(1))  # revealed: Container[int]
```
