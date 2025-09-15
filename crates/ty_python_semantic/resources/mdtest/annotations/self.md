# Self

```toml
[environment]
python-version = "3.13"
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

## Detection of implicit Self

In instance methods, the first parameter (regardless of its name) is assumed to have type
`typing.Self` unless it has an explicit annotation. This does not apply to `@classmethod` and
`@staticmethod`.

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Self

class A:
    def implicit_self(self) -> Self:
        # TODO: first argument in a method should be considered as "typing.Self"
        reveal_type(self)  # revealed: Unknown
        return self

    def foo(self) -> int:
        def first_arg_is_not_self(a: int) -> int:
            return a
        return first_arg_is_not_self(1)

    @classmethod
    def bar(cls): ...
    @staticmethod
    def static(x): ...

a = A()
# TODO: Should reveal Self@implicit_self. Requires implicit self in method body(https://github.com/astral-sh/ruff/pull/18473)
reveal_type(a.implicit_self())  # revealed: A
reveal_type(a.implicit_self)  # revealed: bound method A.implicit_self() -> A
```

If the method is a class or static method then first argument is not self:

```py
A.bar()
a.static(1)
```

"self" name is not special; any first parameter name is treated as Self.

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

class B:
    def implicit_this(this) -> Self:
        # TODO: Should reveal Self@implicit_this
        reveal_type(this)  # revealed: Unknown
        return this

    def ponly(self, /, x: int) -> None:
        # TODO: Should reveal Self@ponly
        reveal_type(self)  # revealed: Unknown

    def kwonly(self, *, x: int) -> None:
        # TODO: Should reveal Self@kwonly
        reveal_type(self)  # revealed: Unknown

    @property
    def name(self) -> str:
        # TODO: Should reveal Self@name
        reveal_type(self)  # revealed: Unknown
        return "b"

B.ponly(B(), 1)
B.name
B.kwonly(B(), x=1)

class G(Generic[T]):
    def id(self) -> Self:
        # TODO: Should reveal Self@id
        reveal_type(self)  # revealed: Unknown
        return self

g = G[int]()

# TODO: Should reveal Self@id Requires implicit self in method body(https://github.com/astral-sh/ruff/pull/18473)
reveal_type(G[int].id(g))  # revealed: G[int]
```

Free functions and nested functions do not use implicit `Self`:

```py
def not_a_method(self):
    reveal_type(self)  # revealed: Unknown

class C:
    def outer(self) -> None:
        def inner(self):
            reveal_type(self)  # revealed: Unknown
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

## `Self` for classes with a default value for their generic parameter

This is a regression test for <https://github.com/astral-sh/ty/issues/1156>.

```py
from typing import Self

class Container[T = bytes]:
    def __init__(self: Self, data: T | None = None) -> None:
        self.data = data

reveal_type(Container())  # revealed: Container[bytes]
reveal_type(Container(1))  # revealed: Container[int]
reveal_type(Container("a"))  # revealed: Container[str]
reveal_type(Container(b"a"))  # revealed: Container[bytes]
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

## Explicit Annotation Overrides Implicit `Self`

If the first parameter is explicitly annotated, that annotation takes precedence over the implicit
`Self` treatment.

```toml
[environment]
python-version = "3.11"
```

```py
class Explicit:
    # TODO: Should warn the user if self is overriden with a type that is not subtype of the class
    def bad(self: int) -> None:
        reveal_type(self)  # revealed: int

    def forward(self: "Explicit") -> None:
        reveal_type(self)  # revealed: Explicit

e = Explicit()
# error: [invalid-argument-type] "Argument to bound method `bad` is incorrect: Expected `int`, found `Explicit`"
e.bad()
```

## Type of Implicit Self

The assigned type to self argument depends on the method signature. When the method is defined in a
non-generic class and has no other mention of `typing.Self` (for example in return type) then type
of `self` is instance of the class.

```py
from typing import Self

class C:
    def f(self) -> Self:
        return self

    def z(self) -> None: ...

C.z(1)  # error: [invalid-argument-type] "Argument to function `z` is incorrect: Expected `C`, found `Literal[1]`"
```

```py
# error: [invalid-argument-type] "Argument to function `f` is incorrect: Argument type `Literal[1]` does not satisfy upper bound `C` of type variable `Self`"
C.f(1)
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

[self attribute]: https://typing.python.org/en/latest/spec/generics.html#use-in-attribute-annotations
