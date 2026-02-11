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

class OuterWithMethod:
    def method(self) -> None:
        class Inner:
            def get(self) -> Self:
                reveal_type(self)  # revealed: Self@get
                return self

            def explicit(self: Self) -> Self:
                reveal_type(self)  # revealed: Self@explicit
                return self

            @classmethod
            def create(cls) -> Self:
                reveal_type(cls)  # revealed: type[Self@create]
                return cls()

            def generic[T](self, x: T) -> Self:
                reveal_type(self)  # revealed: Self@generic
                return self

            def with_nested_function(self) -> Self:
                def helper() -> Self:
                    reveal_type(self)  # revealed: Self@with_nested_function
                    return self
                return helper()

        reveal_type(Inner().get())  # revealed: Inner
        reveal_type(Inner.create())  # revealed: Inner

class DoublyNested:
    def outer_method(self) -> None:
        class Middle:
            def middle_method(self) -> None:
                class Innermost:
                    def get(self) -> Self:
                        reveal_type(self)  # revealed: Self@get
                        return self

def free_function() -> None:
    class Inner:
        def get(self) -> Self:
            reveal_type(self)  # revealed: Self@get
            return self

class OuterWithClassmethod:
    @classmethod
    def factory(cls) -> None:
        class Inner:
            def get(self) -> Self:
                reveal_type(self)  # revealed: Self@get
                return self

            @classmethod
            def create(cls) -> Self:
                reveal_type(cls)  # revealed: type[Self@create]
                return cls()

        reveal_type(Inner().get())  # revealed: Inner
        reveal_type(Inner.create())  # revealed: Inner

class NestedClassExplicitSelf:
    class Bar:
        def method_a(self) -> None:
            def first_param_is_explicit_self(this: Self) -> None:
                reveal_type(this)  # revealed: Self@method_a
                reveal_type(self)  # revealed: Self@method_a
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
        reveal_type(x.foo())  # revealed: Self@bar

def f[U: Bar](x: Foo[U]):
    # revealed: bound method Foo[U@f].foo() -> U@f
    reveal_type(x.foo)
    reveal_type(x.foo())  # revealed: U@f
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

### Calling `super()` in overridden methods with `Self` return type

This is a regression test for <https://github.com/astral-sh/ty/issues/2122>.

When a child class overrides a parent method with a `Self` return type and calls `super().method()`,
the return type should be the child's `Self` type variable, not the concrete child class type.

```py
from typing import Self

class Parent:
    def copy(self) -> Self:
        return self

class Child(Parent):
    def copy(self) -> Self:
        result = super().copy()
        reveal_type(result)  # revealed: Self@copy
        return result

# When called on concrete types, Self is substituted correctly.
reveal_type(Child().copy())  # revealed: Child
```

The same applies to classmethods with `Self` return types:

```py
from typing import Self

class Parent:
    @classmethod
    def create(cls) -> Self:
        return cls()

class Child(Parent):
    @classmethod
    def create(cls) -> Self:
        result = super().create()
        reveal_type(result)  # revealed: Self@create
        return result

# When called on concrete types, Self is substituted correctly.
reveal_type(Child.create())  # revealed: Child
```

## Attributes

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

Dataclass fields can also use `Self` in their annotations:

```py
from dataclasses import dataclass
from typing import Self

@dataclass
class Node:
    parent: Self | None = None

Node(Node())
```

Attributes annotated with `Self` can be assigned on instances:

```py
from typing import Self

class MyClass:
    field: Self | None = None

def _(c: MyClass):
    c.field = c
```

Self from class body annotations and method signatures represent the same logical type variable.
When a method returns an attribute annotated with `Self` in the class body, the class-body `Self`
and the method's `Self` should be considered the same type, even though they have different binding
contexts internally:

```py
from typing import Self

class Chain:
    next: Self
    value: int

    def advance(self: Self) -> Self:
        return self.next

    def advance_twice(self: Self) -> Self:
        return self.advance().advance()

class SubChain(Chain):
    extra: str

reveal_type(SubChain().advance())  # revealed: SubChain
reveal_type(SubChain().advance_twice())  # revealed: SubChain
```

Self-typed attributes that flow through generic containers should also work:

```py
from typing import Self

class TreeNode:
    children: list[Self]
    parent: Self | None

    def first_child(self) -> Self | None:
        if self.children:
            return self.children[0]
        return None

    def all_descendants(self) -> list[Self]:
        result: list[Self] = []
        for child in self.children:
            result.append(child)
            result.extend(child.all_descendants())
        return result

    def root(self) -> Self:
        node = self
        while node.parent is not None:
            node = node.parent
        return node
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

## Callable attributes that return `Self`

Attributes annotated as callables returning `Self` should bind to the concrete class.

```py
from typing import Callable, Self

class Factory:
    maker: Callable[[], Self]

    def __init__(self) -> None:
        self.maker = lambda: self

class Sub(Factory):
    pass

def _(s: Sub):
    reveal_type(s.maker())  # revealed: Sub
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

## Generic class with bounded type variable

This is a regression test for <https://github.com/astral-sh/ty/issues/2467>.

Calling a method on a generic class instance should work when the type parameter is specialized with
a type that satisfies a bound.

```py
from typing import NewType

class Base: ...

class C[T: Base]:
    x: T

    def g(self) -> None:
        pass

# Calling a method on a specialized instance should not produce an error
C[Base]().g()

# Test with a NewType bound
K = NewType("K", int)

class D[T: K]:
    x: T

    def h(self) -> None:
        pass

# Calling a method on a specialized instance should not produce an error
D[K]().h()
```

## Protocols

See also: <https://typing.python.org/en/latest/spec/generics.html#use-in-protocols>

```py
from typing import Self, Protocol

class Copyable(Protocol):
    def copy(self) -> Self: ...

class Linkable(Protocol):
    next_node: Self

    def advance(self) -> Self:
        return self.next_node

def _(l: Linkable) -> None:
    # TODO: Should be `Linkable`
    reveal_type(l.next_node)  # revealed: @Todo(type[T] for protocols)

class CopyableImpl:
    def copy(self) -> Self:
        return self

class SubCopyable(CopyableImpl): ...

def copy_it(x: Copyable) -> None:
    reveal_type(x.copy())  # revealed: Copyable

def copy_concrete(x: CopyableImpl) -> None:
    reveal_type(x.copy())  # revealed: CopyableImpl

def copy_sub(x: SubCopyable) -> None:
    reveal_type(x.copy())  # revealed: SubCopyable
```

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
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def make() -> Self:
        return Foo()

class Bar(Generic[T]): ...

# error: [invalid-type-form]
class Baz(Bar[Self]): ...
```

## Self usage in static methods

`Self` cannot be used anywhere in a static method, including parameters, return types, nested
functions, and default argument values.

```py
from typing import Self

class StaticMethodTests:
    @staticmethod
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def with_self_return() -> Self:
        pass

    @staticmethod
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def with_self_param(x: Self) -> None:
        pass

    @staticmethod
    def with_nested_function() -> None:
        # `Self` in nested function inside static method is also invalid
        # because `Self` binds to the outermost method (the static method).
        # error: [invalid-type-form] "`Self` cannot be used in a static method"
        def inner() -> Self:
            pass

    @staticmethod
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def with_self_default(x: int = 0, y: "Self | None" = None) -> None:
        pass
```

## Aliased staticmethod decorator

Using an aliased `staticmethod` decorator should still be detected:

```py
from typing import Self

sm = staticmethod

class AliasedStaticMethod:
    @sm
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def aliased_static() -> Self:
        pass
```

## `__new__` allows `Self`

`__new__` is a static method even without an explicit `@staticmethod` decorator, but at runtime it
is heavily special-cased by the interpreter to behave more like a classmethod. It always receives a
`cls` parameter with type `type[Self]` and typically returns an object of type `Self`, so `Self` is
permitted in `__new__`:

```py
from typing import Self

class WithNew:
    def __new__(cls) -> Self:
        instance = object.__new__(cls)
        return instance

reveal_type(WithNew())  # revealed: WithNew

class SubclassWithNew(WithNew):
    def __new__(cls) -> Self:
        return super().__new__(cls)

reveal_type(SubclassWithNew())  # revealed: SubclassWithNew
```

## Stacked decorators with staticmethod

When `@staticmethod` is stacked with other decorators, `Self` should still be invalid:

```py
from typing import Self, Callable, TypeVar

T = TypeVar("T")

def identity(f: T) -> T:
    return f

class StackedDecorators:
    @staticmethod
    @identity
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def static_then_identity() -> Self:
        pass
    # TODO: On Python <3.10, this should ideally be rejected, because `staticmethod` objects were not callable.
    @identity
    @staticmethod
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def identity_then_static() -> Self:
        pass
```

## Self usage in metaclasses

`Self` cannot be used in a metaclass because the semantics are confusing: in a metaclass, `self`
refers to a class (the metaclass instance), not a regular object instance.

```py
from typing import Self

class MyMetaclass(type):
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    registry: list[Self]

    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def __new__(cls, name, bases, dct) -> Self:
        return cls(name, bases, dct)
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def instance_method(self) -> Self:
        return self

    @classmethod
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def metaclass_classmethod(cls) -> Self:
        return cls("", (), {})
    # Note: static methods in metaclasses get the static method error, not metaclass error
    @staticmethod
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def metaclass_staticmethod() -> Self:
        pass
```

## Runtime use of `self` parameter in metaclass

Using the `self` parameter as a runtime value (e.g. in `Union[self, other]`) should not be flagged,
even in a metaclass. Only the literal `Self` type form should be disallowed.

```py
from typing import Union

class AnnotableMeta(type):
    def __or__(self, other):
        return Union[self, other]  # No error: runtime use of `self`, not the `Self` type form
```

## Indirect metaclass inheritance

Classes that inherit from `type` indirectly (through another metaclass) are also metaclasses:

```py
from typing import Self
from abc import ABCMeta

class IndirectMetaclass(ABCMeta):
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def method(self) -> Self:
        return self

class MultiLevelMeta(IndirectMetaclass):
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def another_method(self) -> Self:
        return self
```

## Classes using a metaclass are not metaclasses

A class that uses a metaclass (via `metaclass=...`) is _not_ itself a metaclass. `Self` should be
valid in such classes:

```py
from typing import Self

class SomeMeta(type):
    pass

class UsesMetaclass(metaclass=SomeMeta):
    def method(self) -> Self:
        reveal_type(self)  # revealed: Self@method
        return self

reveal_type(UsesMetaclass().method())  # revealed: UsesMetaclass

class SubclassOfMetaclassUser(UsesMetaclass):
    def another(self) -> Self:
        return self

reveal_type(SubclassOfMetaclassUser().another())  # revealed: SubclassOfMetaclassUser
```

## Nested class inside a metaclass

A nested class inside a metaclass is _not_ a metaclass (unless it also inherits from `type`):

```py
from typing import Self

class OuterMeta(type):
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def meta_method(self) -> Self:
        return self

    class NestedRegularClass:
        # This is fine - NestedRegularClass is not a metaclass
        def method(self) -> Self:
            reveal_type(self)  # revealed: Self@method
            return self

    class NestedMetaclass(type):
        # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
        def nested_meta_method(self) -> Self:
            return self
```

## `builtins.staticmethod`

Using the fully qualified `builtins.staticmethod` should also be detected:

```py
from typing import Self
import builtins

class BuiltinsStaticMethod:
    @builtins.staticmethod
    # error: [invalid-type-form] "`Self` cannot be used in a static method"
    def method() -> Self:
        pass
```

## EnumMeta is a metaclass

`enum.EnumMeta` (or `enum.EnumType` in Python 3.11+) is a metaclass, so `Self` should be invalid:

```py
from typing import Self
from enum import EnumMeta

class CustomEnumMeta(EnumMeta):
    # error: [invalid-type-form] "`Self` cannot be used in a metaclass"
    def custom_method(self) -> Self:
        return self
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

class C[T]():  # fmt:skip
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

## Bound methods stored as instance attributes

Bound methods from other objects stored as instance attributes should not have their signatures
affected by `Self` type binding. This is a regression test for false positives in projects like
jinja's `LRUCache`.

```py
from collections import deque

class MyClass:
    def __init__(self) -> None:
        self._queue: deque[int] = deque()
        self._append = self._queue.append

    def add(self, value: int) -> None:
        self._append(value)
```

## Self in class attributes with generic classes

Django-like patterns where a class attribute uses `Self` as a type argument to a generic class. Both
class access (`Confirmation.objects`) and instance access (`instance.objects`) should properly bind
`Self` to the concrete class.

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

class Manager(Generic[T]):
    def get(self) -> T:
        raise NotImplementedError

class Model:
    objects: Manager[Self]

class Confirmation(Model):
    expiry_date: int

def test() -> None:
    # Class access: Self is bound to Confirmation
    confirmation = Confirmation.objects.get()
    reveal_type(confirmation)  # revealed: Confirmation
    x = confirmation.expiry_date  # Should work - Confirmation has expiry_date

    # Instance access: Self should also be bound to Confirmation
    instance = Confirmation()
    reveal_type(instance.objects)  # revealed: Manager[Confirmation]
    instance_result = instance.objects.get()
    reveal_type(instance_result)  # revealed: Confirmation
```

## Self in class attributes with descriptors

`Self` binding should also work when the attribute type involves a descriptor.

```py
from typing import Self, Generic, TypeVar

T = TypeVar("T")

class Descriptor(Generic[T]):
    def __get__(self, instance, owner) -> T:
        raise NotImplementedError

class Base:
    attr: Descriptor[Self] = Descriptor()

class Child(Base):
    pass

reveal_type(Child.attr)  # revealed: Child
reveal_type(Child().attr)  # revealed: Child
```
