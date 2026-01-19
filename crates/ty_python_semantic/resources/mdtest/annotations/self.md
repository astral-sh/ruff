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
from typing import Optional, Self

@dataclass
class Node:
    parent: Optional[Self] = None

Node(Node())
```

Attributes annotated with `Self` can be assigned on instances:

```py
from typing import Optional, Self

class MyClass:
    field: Optional[Self] = None

def _(c: MyClass):
    c.field = c
```

Accessing base class attributes through `Self` should work correctly. This is a common pattern in
ORMs like Django where a base `Model` class defines methods that access `self` attributes, and those
methods are inherited by subclasses:

```py
from typing import Self

class Model:
    id: int
    name: str

    def get_id(self: Self) -> int:
        # Self is bound by Model here, but should still find id
        reveal_type(self.id)  # revealed: int
        return self.id

class User(Model):
    email: str

reveal_type(User().get_id())  # revealed: int
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
    def __new__(cls, name, bases, dct) -> Self:
        return cls(name, bases, dct)
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

## Bound methods from internal data structures stored as instance attributes

This tests the pattern where a class stores bound methods from internal data structures (like
`deque` or `dict`) as instance attributes for performance. When these bound methods are later
accessed and called through `self`, the `Self` type binding should not interfere with their
signatures.

This is a regression test for false positives found in ecosystem projects like jinja's `LRUCache`
and beartype's `CacheUnboundedStrong`.

```py
from collections import deque
from typing import Any

class LRUCache:
    """A simple LRU cache that stores bound methods from an internal deque."""

    def __init__(self, capacity: int) -> None:
        self.capacity = capacity
        self._mapping: dict[Any, Any] = {}
        self._queue: deque[Any] = deque()
        self._postinit()

    def _postinit(self) -> None:
        # Store bound methods from the internal deque for faster attribute lookup
        self._popleft = self._queue.popleft
        self._pop = self._queue.pop
        self._remove = self._queue.remove
        self._append = self._queue.append

    def __getitem__(self, key: Any) -> Any:
        # These should not produce errors - the bound methods have signatures
        # from deque, not involving Self
        self._remove(key)
        self._append(key)
        return self._mapping[key]

    def __setitem__(self, key: Any, value: Any) -> None:
        self._remove(key)
        if len(self._queue) >= self.capacity:
            self._popleft()
        self._append(key)
        self._mapping[key] = value

    def __delitem__(self, key: Any) -> None:
        self._remove(key)
        del self._mapping[key]
```

Similarly for dict-based patterns:

```py
from typing import Hashable

class CacheMap:
    """A cache that stores bound methods from an internal dict."""

    def __init__(self) -> None:
        self._key_to_value: dict[Hashable, object] = {}
        self._key_to_value_get = self._key_to_value.get
        self._key_to_value_set = self._key_to_value.__setitem__

    def cache_or_get_cached_value(self, key: Hashable, value: object) -> object:
        # This should not produce errors - we're using dict's get/setitem methods
        cached_value = self._key_to_value_get(key)
        if cached_value is not None:
            return cached_value
        self._key_to_value_set(key, value)
        return value
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

## Limitation: Stubs that rely on type checker plugins

Some stub packages like `django-stubs` rely on mypy plugins to synthesize attributes that don't
exist in the stubs themselves. For example, Django's `Model` class automatically gets an `id: int`
field at runtime, but `django-stubs` doesn't include this in the stub because the mypy plugin adds
it dynamically based on the model definition.

Without plugin support, type checkers (including ty, mypy, and pyright) will correctly report that
the attribute doesn't exist. This is the expected behavior - the stubs are incomplete without the
plugin.

```py
from typing import Self, Generic, TypeVar, ClassVar, Any

_T = TypeVar("_T", bound="Model", covariant=True)

class Manager(Generic[_T]):
    def create(self, **kwargs: Any) -> _T:
        raise NotImplementedError

class Model:
    # django-stubs defines pk: Any but NOT id: int
    # The id field is supposed to be synthesized by the mypy plugin
    pk: Any
    objects: ClassVar[Manager[Self]]

class CustomerPlan(Model):
    name: str

plan = CustomerPlan.objects.create(name="test")

# Self binding works correctly - plan is CustomerPlan, not Unknown
reveal_type(plan)  # revealed: CustomerPlan

# pk works because it's defined in Model
reveal_type(plan.pk)  # revealed: Any

# id fails because it's not in the stubs (requires mypy plugin)
# error: [unresolved-attribute]
plan.id
```

The workaround is to add the missing attributes to a custom base class:

```py
from typing import Self, Generic, TypeVar, ClassVar, Any

_T = TypeVar("_T", bound="Model", covariant=True)

class Manager(Generic[_T]):
    def create(self, **kwargs: Any) -> _T:
        raise NotImplementedError

class Model:
    id: int  # Explicitly add id to work without mypy plugin
    pk: Any
    objects: ClassVar[Manager[Self]]

class CustomerPlan(Model):
    name: str

plan = CustomerPlan.objects.create(name="test")
reveal_type(plan)  # revealed: CustomerPlan
reveal_type(plan.id)  # revealed: int
```
