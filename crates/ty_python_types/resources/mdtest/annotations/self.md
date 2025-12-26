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
            reveal_type(x)  # revealed: Self@nested_func_without_enclosing_binding
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

## Type of (unannotated) `self` parameters

In instance methods, the first parameter (regardless of its name) is assumed to have the type
`typing.Self`, unless it has an explicit annotation. This does not apply to `@classmethod` and
`@staticmethod`s.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Self

class A:
    def __init__(self):
        reveal_type(self)  # revealed: Self@__init__

    def __init_subclass__(cls, default_name, **kwargs):
        reveal_type(cls)  # revealed: type[Self@__init_subclass__]

    def implicit_self(self) -> Self:
        reveal_type(self)  # revealed: Self@implicit_self

        return self

    def implicit_self_generic[T](self, x: T) -> T:
        reveal_type(self)  # revealed: Self@implicit_self_generic

        return x

    def method_a(self) -> None:
        def first_param_is_not_self(a: int):
            reveal_type(a)  # revealed: int
            reveal_type(self)  # revealed: Self@method_a

        def first_param_is_not_self_unannotated(a):
            reveal_type(a)  # revealed: Unknown
            reveal_type(self)  # revealed: Self@method_a

        def first_param_is_also_not_self(self) -> None:
            reveal_type(self)  # revealed: Unknown

        def first_param_is_explicit_self(this: Self) -> None:
            reveal_type(this)  # revealed: Self@method_a
            reveal_type(self)  # revealed: Self@method_a

    @classmethod
    def a_classmethod(cls) -> Self:
        reveal_type(cls)  # revealed: type[Self@a_classmethod]
        return cls()

    @staticmethod
    def a_staticmethod(x: int): ...

a = A()

reveal_type(a.implicit_self())  # revealed: A
reveal_type(a.implicit_self)  # revealed: bound method A.implicit_self() -> A
```

Calling an instance method explicitly verifies the first argument:

```py
A.implicit_self(a)

# error: [invalid-argument-type] "Argument to function `implicit_self` is incorrect: Argument type `Literal[1]` does not satisfy upper bound `A` of type variable `Self`"
A.implicit_self(1)
```

Passing `self` implicitly also verifies the type:

```py
from typing import Never, Callable

class Strange:
    def can_not_be_called(self: Never) -> None: ...

# error: [invalid-argument-type] "Argument to bound method `can_not_be_called` is incorrect: Expected `Never`, found `Strange`"
Strange().can_not_be_called()
```

If the method is a class or static method then first argument is not inferred as `Self`:

```py
A.a_classmethod()
A.a_classmethod(a)  # error: [too-many-positional-arguments]
A.a_staticmethod(1)
a.a_staticmethod(1)
A.a_staticmethod(a)  # error: [invalid-argument-type]
```

The first parameter of instance methods always has type `Self`, if it is not explicitly annotated.
The name `self` is not special in any way.

```py
def some_decorator[**P, R](f: Callable[P, R]) -> Callable[P, R]:
    return f

class B:
    def name_does_not_matter(this) -> Self:
        reveal_type(this)  # revealed: Self@name_does_not_matter

        return this

    def positional_only(self, /, x: int) -> Self:
        reveal_type(self)  # revealed: Self@positional_only
        return self

    def keyword_only(self, *, x: int) -> Self:
        reveal_type(self)  # revealed: Self@keyword_only
        return self

    @some_decorator
    def decorated_method(self) -> Self:
        reveal_type(self)  # revealed: Self@decorated_method
        return self

    @property
    def a_property(self) -> Self:
        reveal_type(self)  # revealed: Self@a_property
        return self

    async def async_method(self) -> Self:
        reveal_type(self)  # revealed: Self@async_method
        return self

    @staticmethod
    def static_method(self):
        # The parameter can be called `self`, but it is not treated as `Self`
        reveal_type(self)  # revealed: Unknown

    @staticmethod
    @some_decorator
    def decorated_static_method(self):
        reveal_type(self)  # revealed: Unknown
    # TODO: On Python <3.10, this should ideally be rejected, because `staticmethod` objects were not callable.
    @some_decorator
    @staticmethod
    def decorated_static_method_2(self):
        reveal_type(self)  # revealed: Unknown

reveal_type(B().name_does_not_matter())  # revealed: B
reveal_type(B().positional_only(1))  # revealed: B
reveal_type(B().keyword_only(x=1))  # revealed: B
reveal_type(B().decorated_method())  # revealed: B

reveal_type(B().a_property)  # revealed: B

async def _():
    reveal_type(await B().async_method())  # revealed: B
```

This also works for generic classes:

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

class G(Generic[T]):
    def id(self) -> Self:
        reveal_type(self)  # revealed: Self@id

        return self

reveal_type(G[int]().id())  # revealed: G[int]
reveal_type(G[str]().id())  # revealed: G[str]
```

Free functions and nested functions do not use implicit `Self`:

```py
def not_a_method(self):
    reveal_type(self)  # revealed: Unknown

# error: [invalid-type-form]
def does_not_return_self(self) -> Self:
    return self

class C:
    def outer(self) -> None:
        def inner(self):
            reveal_type(self)  # revealed: Unknown

reveal_type(not_a_method)  # revealed: def not_a_method(self) -> Unknown
```

## Different occurrences of `Self` represent different types

Here, both `Foo.foo` and `Bar.bar` use `Self`. When accessing a bound method, we replace any
occurrences of `Self` with the bound `self` type. In this example, when we access `x.foo`, we only
want to substitute the occurrences of `Self` in `Foo.foo` — that is, occurrences of `Self@foo`. The
fact that `x` is an instance of `Foo[Self@bar]` (a completely different `Self` type) should not
affect that subtitution. If we blindly substitute all occurrences of `Self`, we would get
`Foo[Self@bar]` as the return type of the bound method.

```py
from typing import Self

class Foo[T]:
    def foo(self: Self) -> T:
        raise NotImplementedError

class Bar:
    def bar(self: Self, x: Foo[Self]):
        # revealed: bound method Foo[Self@bar].foo() -> Self@bar
        reveal_type(x.foo)

def f[U: Bar](x: Foo[U]):
    # revealed: bound method Foo[U@f].foo() -> U@f
    reveal_type(x.foo)
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

### Explicit

```py
from typing import Self

class Shape:
    def foo(self: Self) -> Self:
        return self

    @classmethod
    def bar(cls: type[Self]) -> Self:
        reveal_type(cls)  # revealed: type[Self@bar]
        return cls()

class Circle(Shape): ...

reveal_type(Shape().foo())  # revealed: Shape
reveal_type(Shape.bar())  # revealed: Shape

reveal_type(Circle().foo())  # revealed: Circle
reveal_type(Circle.bar())  # revealed: Circle
```

### Implicit

```py
from typing import Self

class Shape:
    def foo(self) -> Self:
        return self

    @classmethod
    def bar(cls) -> Self:
        reveal_type(cls)  # revealed: type[Self@bar]
        return cls()

class Circle(Shape): ...

reveal_type(Shape().foo())  # revealed: Shape
reveal_type(Shape.bar())  # revealed: Shape

reveal_type(Circle().foo())  # revealed: Circle
reveal_type(Circle.bar())  # revealed: Circle
```

### Implicit in generic class

```py
from typing import Self

class GenericShape[T]:
    def foo(self) -> Self:
        return self

    @classmethod
    def bar(cls) -> Self:
        reveal_type(cls)  # revealed: type[Self@bar]
        return cls()

    @classmethod
    def baz[U](cls, u: U) -> "GenericShape[U]":
        reveal_type(cls)  # revealed: type[Self@baz]
        return cls()

class GenericCircle[T](GenericShape[T]): ...

reveal_type(GenericShape().foo())  # revealed: GenericShape[Unknown]
reveal_type(GenericShape.bar())  # revealed: GenericShape[Unknown]
reveal_type(GenericShape[int].bar())  # revealed: GenericShape[int]
reveal_type(GenericShape.baz(1))  # revealed: GenericShape[Literal[1]]

reveal_type(GenericCircle().foo())  # revealed: GenericCircle[Unknown]
reveal_type(GenericCircle.bar())  # revealed: GenericCircle[Unknown]
reveal_type(GenericCircle[int].bar())  # revealed: GenericCircle[int]
reveal_type(GenericCircle.baz(1))  # revealed: GenericShape[Literal[1]]
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

Attributes can also refer to a generic parameter:

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    foo: T
    def method(self) -> None:
        reveal_type(self)  # revealed: Self@method
        reveal_type(self.foo)  # revealed: T@C
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

## Implicit self for classes with a default value for their generic parameter

```py
from typing import Self, TypeVar, Generic

class Container[T = bytes]:
    def method(self) -> Self:
        return self

def _(c: Container[str], d: Container):
    reveal_type(c.method())  # revealed: Container[str]
    reveal_type(d.method())  # revealed: Container[bytes]

T = TypeVar("T", default=bytes)

class LegacyContainer(Generic[T]):
    def method(self) -> Self:
        return self

def _(c: LegacyContainer[str], d: LegacyContainer):
    reveal_type(c.method())  # revealed: LegacyContainer[str]
    reveal_type(d.method())  # revealed: LegacyContainer[bytes]
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
    # TODO: This `self: T` annotation should be rejected because `T` is not `Self`
    def has_existing_self_annotation(self: T) -> Self:
        return self  # error: [invalid-return-type]

    def return_concrete_type(self) -> Self:
        # TODO: We could emit a hint that suggests annotating with `Foo` instead of `Self`
        # error: [invalid-return-type]
        return Foo()

    @staticmethod
    # TODO: The usage of `Self` here should be rejected because this is a static method
    def make() -> Self:
        # error: [invalid-return-type]
        return Foo()

class Bar(Generic[T]): ...

# error: [invalid-type-form]
class Baz(Bar[Self]): ...

class MyMetaclass(type):
    # TODO: reject the Self usage. because self cannot be used within a metaclass.
    def __new__(cls) -> Self:
        return super().__new__(cls)
```

## Explicit annotations override implicit `Self`

If the first parameter is explicitly annotated, that annotation takes precedence over the implicit
`Self` type.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

from typing import final

@final
class Disjoint: ...

class Explicit:
    # TODO: We could emit a warning if the annotated type of `self` is disjoint from `Explicit`
    def bad(self: Disjoint) -> None:
        reveal_type(self)  # revealed: Disjoint

    def forward(self: Explicit) -> None:
        reveal_type(self)  # revealed: Explicit

# error: [invalid-argument-type] "Argument to bound method `bad` is incorrect: Expected `Disjoint`, found `Explicit`"
Explicit().bad()

Explicit().forward()

class ExplicitGeneric[T]:
    def special(self: ExplicitGeneric[int]) -> None:
        reveal_type(self)  # revealed: ExplicitGeneric[int]

ExplicitGeneric[int]().special()

# TODO: this should be an `invalid-argument-type` error
ExplicitGeneric[str]().special()
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

In nested functions `self` binds to the method. So in the following example the `self` in `C.b` is
bound at `C.f`.

```py
from typing import Self
from ty_extensions import generic_context

class C[T]():
    def f(self: Self):
        def b(x: Self):
            reveal_type(x)  # revealed: Self@f
        # revealed: None
        reveal_type(generic_context(b))

# revealed: ty_extensions.GenericContext[Self@f]
reveal_type(generic_context(C.f))
```

Even if the `Self` annotation appears first in the nested function, it is the method that binds
`Self`.

```py
from typing import Self
from ty_extensions import generic_context

class C:
    def f(self: "C"):
        def b(x: Self):
            reveal_type(x)  # revealed: Self@f
        # revealed: None
        reveal_type(generic_context(b))

# revealed: None
reveal_type(generic_context(C.f))
```

## Non-positional first parameters

This makes sure that we don't bind `self` if it's not a positional parameter:

```py
from ty_extensions import CallableTypeOf

class C:
    def method(*args, **kwargs) -> None: ...

def _(c: CallableTypeOf[C().method]):
    reveal_type(c)  # revealed: (...) -> None
```

[self attribute]: https://typing.python.org/en/latest/spec/generics.html#use-in-attribute-annotations
