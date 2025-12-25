# Super

Python defines the terms _bound super object_ and _unbound super object_.

An **unbound super object** is created when `super` is called with only one argument. (e.g.
`super(A)`). This object may later be bound using the `super.__get__` method. However, this form is
rarely used in practice.

A **bound super object** is created either by calling `super(pivot_class, owner)` or by using the
implicit form `super()`, where both the pivot class and the owner are inferred. This is the most
common usage.

## Basic Usage

### Explicit Super Object

<!-- snapshot-diagnostics -->

`super(pivot_class, owner)` performs attribute lookup along the MRO, starting immediately after the
specified pivot class.

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations
from ty_extensions import reveal_mro

class A:
    def a(self): ...
    aa: int = 1

class B(A):
    def b(self): ...
    bb: int = 2

class C(B):
    def c(self): ...
    cc: int = 3

reveal_mro(C)  # revealed: (<class 'C'>, <class 'B'>, <class 'A'>, <class 'object'>)

super(C, C()).a
super(C, C()).b
super(C, C()).c  # error: [unresolved-attribute]

super(B, C()).a
super(B, C()).b  # error: [unresolved-attribute]
super(B, C()).c  # error: [unresolved-attribute]

super(A, C()).a  # error: [unresolved-attribute]
super(A, C()).b  # error: [unresolved-attribute]
super(A, C()).c  # error: [unresolved-attribute]

reveal_type(super(C, C()).a)  # revealed: bound method C.a() -> Unknown
reveal_type(super(C, C()).b)  # revealed: bound method C.b() -> Unknown
reveal_type(super(C, C()).aa)  # revealed: int
reveal_type(super(C, C()).bb)  # revealed: int
```

Examples of explicit `super()` with unusual types. We allow almost any type to be passed as the
second argument to `super()` -- the only exceptions are "pure abstract" types such as `Callable` and
synthesized `Protocol`s that cannot be upcast to, or interpreted as, a non-`object` nominal type.

```py
import types
from typing_extensions import Callable, TypeIs, Literal, NewType, TypedDict

def f(): ...

class Foo[T]:
    def method(self): ...
    @property
    def some_property(self): ...

type Alias = int

class SomeTypedDict(TypedDict):
    x: int
    y: bytes

N = NewType("N", int)

# revealed: <super: <class 'object'>, FunctionType>
reveal_type(super(object, f))
# revealed: <super: <class 'object'>, WrapperDescriptorType>
reveal_type(super(object, types.FunctionType.__get__))
# revealed: <super: <class 'object'>, GenericAlias>
reveal_type(super(object, Foo[int]))
# revealed: <super: <class 'object'>, _SpecialForm>
reveal_type(super(object, Literal))
# revealed: <super: <class 'object'>, TypeAliasType>
reveal_type(super(object, Alias))
# revealed: <super: <class 'object'>, MethodType>
reveal_type(super(object, Foo().method))
# revealed: <super: <class 'object'>, property>
reveal_type(super(object, Foo.some_property))
# revealed: <super: <class 'object'>, int>
reveal_type(super(object, N(42)))

def g(x: object) -> TypeIs[list[object]]:
    return isinstance(x, list)

def _(x: object, y: SomeTypedDict, z: Callable[[int, str], bool]):
    if hasattr(x, "bar"):
        # revealed: <Protocol with members 'bar'>
        reveal_type(x)
        # error: [invalid-super-argument]
        # revealed: Unknown
        reveal_type(super(object, x))

    # error: [invalid-super-argument]
    # revealed: Unknown
    reveal_type(super(object, z))

    is_list = g(x)
    # revealed: TypeIs[list[object] @ x]
    reveal_type(is_list)
    # revealed: <super: <class 'object'>, bool>
    reveal_type(super(object, is_list))

    # revealed: <super: <class 'object'>, dict[Literal["x", "y"], int | bytes]>
    reveal_type(super(object, y))

# The first argument to `super()` must be an actual class object;
# instances of `GenericAlias` are not accepted at runtime:
#
# error: [invalid-super-argument]
# revealed: Unknown
reveal_type(super(list[int], []))
```

`super(pivot_class, owner)` can be called from inside methods, just like single-argument `super()`:

```py
class Super:
    def method(self) -> int:
        return 42

class Sub(Super):
    def method(self: Sub) -> int:
        # revealed: <super: <class 'Sub'>, Sub>
        return reveal_type(super(self.__class__, self)).method()
```

### Implicit Super Object

<!-- snapshot-diagnostics -->

The implicit form `super()` is same as `super(__class__, <first argument>)`. The `__class__` refers
to the class that contains the function where `super()` is used. The first argument refers to the
current method’s first parameter (typically `self` or `cls`).

```toml
[environment]
python-version = "3.12"
```

```py
from __future__ import annotations

class A:
    def __init__(self, a: int): ...
    @classmethod
    def f(cls): ...

class B(A):
    def __init__(self, a: int):
        reveal_type(super())  # revealed: <super: <class 'B'>, B>
        reveal_type(super(object, super()))  # revealed: <super: <class 'object'>, super>
        super().__init__(a)

    @classmethod
    def f(cls):
        reveal_type(super())  # revealed: <super: <class 'B'>, <class 'B'>>
        super().f()

super(B, B(42)).__init__(42)
super(B, B).f()
```

Some examples with unusual annotations for `self` or `cls`:

```py
import enum
from typing import Any, Self, Never, Protocol, Callable
from ty_extensions import Intersection

class BuilderMeta(type):
    def __new__(
        cls: type[Any],
        name: str,
        bases: tuple[type, ...],
        dct: dict[str, Any],
    ) -> BuilderMeta:
        # revealed: <super: <class 'BuilderMeta'>, Any>
        s = reveal_type(super())
        # revealed: Any
        return reveal_type(s.__new__(cls, name, bases, dct))

class BuilderMeta2(type):
    def __new__(
        cls: type[BuilderMeta2],
        name: str,
        bases: tuple[type, ...],
        dct: dict[str, Any],
    ) -> BuilderMeta2:
        # revealed: <super: <class 'BuilderMeta2'>, <class 'BuilderMeta2'>>
        s = reveal_type(super())
        return reveal_type(s.__new__(cls, name, bases, dct))  # revealed: BuilderMeta2

class Foo[T]:
    x: T

    def method(self: Any):
        reveal_type(super())  # revealed: <super: <class 'Foo'>, Any>

        if isinstance(self, Foo):
            reveal_type(super())  # revealed: <super: <class 'Foo'>, Any>

    def method2(self: Foo[T]):
        # revealed: <super: <class 'Foo'>, Foo[T@Foo]>
        reveal_type(super())

    def method3(self: Foo):
        # revealed: <super: <class 'Foo'>, Foo[Unknown]>
        reveal_type(super())

    def method4(self: Self):
        # revealed: <super: <class 'Foo'>, Foo[T@Foo]>
        reveal_type(super())

    def method5[S: Foo[int]](self: S, other: S) -> S:
        # revealed: <super: <class 'Foo'>, Foo[int]>
        reveal_type(super())
        return self

    def method6[S: (Foo[int], Foo[str])](self: S, other: S) -> S:
        # revealed: <super: <class 'Foo'>, Foo[int]> | <super: <class 'Foo'>, Foo[str]>
        reveal_type(super())
        return self

    def method7[S](self: S, other: S) -> S:
        # error: [invalid-super-argument]
        # revealed: Unknown
        reveal_type(super())
        return self

    def method8[S: int](self: S, other: S) -> S:
        # error: [invalid-super-argument]
        # revealed: Unknown
        reveal_type(super())
        return self

    def method9[S: (int, str)](self: S, other: S) -> S:
        # error: [invalid-super-argument]
        # revealed: Unknown
        reveal_type(super())
        return self

    def method10[S: Callable[..., str]](self: S, other: S) -> S:
        # error: [invalid-super-argument]
        # revealed: Unknown
        reveal_type(super())
        return self

type Alias = Bar

class Bar:
    def method(self: Alias):
        # revealed: <super: <class 'Bar'>, Bar>
        reveal_type(super())

    def pls_dont_call_me(self: Never):
        # revealed: <super: <class 'Bar'>, Unknown>
        reveal_type(super())

    def only_call_me_on_callable_subclasses(self: Intersection[Bar, Callable[..., object]]):
        # revealed: <super: <class 'Bar'>, Bar>
        reveal_type(super())

class P(Protocol):
    def method(self: P):
        # revealed: <super: <class 'P'>, P>
        reveal_type(super())

class E(enum.Enum):
    X = 1

    def method(self: E):
        match self:
            case E.X:
                # revealed: <super: <class 'E'>, E>
                reveal_type(super())
```

### Unbound Super Object

Calling `super(cls)` without a second argument returns an _unbound super object_. This is treated as
a plain `super` instance and does not support name lookup via the MRO.

```py
class A:
    a: int = 42

class B(A): ...

reveal_type(super(B))  # revealed: super

# error: [unresolved-attribute] "Object of type `super` has no attribute `a`"
super(B).a
```

## Attribute Assignment

`super()` objects do not allow attribute assignment — even if the attribute is resolved
successfully.

```py
class A:
    a: int = 3

class B(A): ...

reveal_type(super(B, B()).a)  # revealed: int
# error: [invalid-assignment] "Cannot assign to attribute `a` on type `<super: <class 'B'>, B>`"
super(B, B()).a = 3
# error: [invalid-assignment] "Cannot assign to attribute `a` on type `super`"
super(B).a = 5
```

## Dynamic Types

If any of the arguments is dynamic, we cannot determine the MRO to traverse. When accessing a
member, it should effectively behave like a dynamic type.

```py
class A:
    a: int = 1

def f(x):
    reveal_type(x)  # revealed: Unknown

    reveal_type(super(x, x))  # revealed: <super: Unknown, Unknown>
    reveal_type(super(A, x))  # revealed: <super: <class 'A'>, Unknown>
    reveal_type(super(x, A()))  # revealed: <super: Unknown, A>

    reveal_type(super(x, x).a)  # revealed: Unknown
    reveal_type(super(A, x).a)  # revealed: Unknown
    reveal_type(super(x, A()).a)  # revealed: Unknown
```

## Implicit `super()` in Complex Structure

```py
from __future__ import annotations

class A:
    def test(self):
        reveal_type(super())  # revealed: <super: <class 'A'>, A>

    class B:
        def test(self):
            reveal_type(super())  # revealed: <super: <class 'B'>, B>

            class C(A.B):
                def test(self):
                    reveal_type(super())  # revealed: <super: <class 'C'>, C>

            def inner(t: C):
                reveal_type(super())  # revealed: <super: <class 'B'>, C>
            lambda x: reveal_type(super())  # revealed: <super: <class 'B'>, Unknown>
```

## Built-ins and Literals

```py
from enum import Enum

reveal_type(super(bool, True))  # revealed: <super: <class 'bool'>, bool>
reveal_type(super(bool, bool()))  # revealed: <super: <class 'bool'>, bool>
reveal_type(super(int, bool()))  # revealed: <super: <class 'int'>, bool>
reveal_type(super(int, 3))  # revealed: <super: <class 'int'>, int>
reveal_type(super(str, ""))  # revealed: <super: <class 'str'>, str>
reveal_type(super(bytes, b""))  # revealed: <super: <class 'bytes'>, bytes>

class E(Enum):
    X = 42

reveal_type(super(E, E.X))  # revealed: <super: <class 'E'>, E>
```

## `type[Self]`

```py
class Foo:
    def method(self):
        super(self.__class__, self)
```

## Descriptor Behavior with Super

Accessing attributes through `super` still invokes descriptor protocol. However, the behavior can
differ depending on whether the second argument to `super` is a class or an instance.

```py
class A:
    def a1(self): ...
    @classmethod
    def a2(cls): ...

class B(A): ...

# A.__dict__["a1"].__get__(B(), B)
reveal_type(super(B, B()).a1)  # revealed: bound method B.a1() -> Unknown
# A.__dict__["a2"].__get__(B(), B)
reveal_type(super(B, B()).a2)  # revealed: bound method type[B].a2() -> Unknown

# A.__dict__["a1"].__get__(None, B)
reveal_type(super(B, B).a1)  # revealed: def a1(self) -> Unknown
# A.__dict__["a2"].__get__(None, B)
reveal_type(super(B, B).a2)  # revealed: bound method <class 'B'>.a2() -> Unknown
```

## Union of Supers

When the owner is a union type, `super()` is built separately for each branch, and the resulting
super objects are combined into a union.

```py
from ty_extensions import reveal_mro

class A: ...

class B:
    b: int = 42

class C(A, B): ...
class D(B, A): ...

def f(x: C | D):
    reveal_mro(C)  # revealed: (<class 'C'>, <class 'A'>, <class 'B'>, <class 'object'>)
    reveal_mro(D)  # revealed: (<class 'D'>, <class 'B'>, <class 'A'>, <class 'object'>)

    s = super(A, x)
    reveal_type(s)  # revealed: <super: <class 'A'>, C> | <super: <class 'A'>, D>

    # error: [possibly-missing-attribute] "Attribute `b` may be missing on object of type `<super: <class 'A'>, C> | <super: <class 'A'>, D>`"
    s.b

def f(flag: bool):
    x = str() if flag else str("hello")
    reveal_type(x)  # revealed: Literal["", "hello"]
    reveal_type(super(str, x))  # revealed: <super: <class 'str'>, str>

def f(x: int | str):
    # error: [invalid-super-argument] "`str` is not an instance or subclass of `<class 'int'>` in `super(<class 'int'>, str)` call"
    super(int, x)
```

Even when `super()` is constructed separately for each branch of a union, it should behave correctly
in all cases.

```py
def f(flag: bool):
    if flag:
        class A:
            x = 1
            y: int = 1

            a: str = "hello"

        class B(A): ...
        s = super(B, B())
    else:
        class C:
            x = 2
            y: int | str = "test"

        class D(C): ...
        s = super(D, D())

    reveal_type(s)  # revealed: <super: <class 'B'>, B> | <super: <class 'D'>, D>

    reveal_type(s.x)  # revealed: Unknown | Literal[1, 2]
    reveal_type(s.y)  # revealed: int | str

    # error: [possibly-missing-attribute] "Attribute `a` may be missing on object of type `<super: <class 'B'>, B> | <super: <class 'D'>, D>`"
    reveal_type(s.a)  # revealed: str
```

## Supers with Generic Classes

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import TypeOf, static_assert, is_subtype_of

class A[T]:
    def f(self, a: T) -> T:
        return a

class B[T](A[T]):
    def f(self, a: T) -> T:
        return super().f(a)
```

## Invalid Usages

### Unresolvable `super()` Calls

If an appropriate class and argument cannot be found, a runtime error will occur.

```py
from __future__ import annotations

# error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
reveal_type(super())  # revealed: Unknown

def f():
    # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
    super()

# No first argument in its scope
class A:
    # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
    s = super()

    def f(self):
        def g():
            # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
            super()
        # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
        lambda: super()

        # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
        (super() for _ in range(10))

    @staticmethod
    def h():
        # error: [unavailable-implicit-super-arguments] "Cannot determine implicit arguments for 'super()' in this context"
        super()
```

### Failing Condition Checks

```toml
[environment]
python-version = "3.12"
```

`super()` requires its first argument to be a valid class, and its second argument to be either an
instance or a subclass of the first. If either condition is violated, a `TypeError` is raised at
runtime.

```py
import typing
import collections

def f(x: int):
    # error: [invalid-super-argument] "`int` is not a valid class"
    super(x, x)

    type IntAlias = int
    # error: [invalid-super-argument] "`TypeAliasType` is not a valid class"
    super(IntAlias, 0)

# error: [invalid-super-argument] "`str` is not an instance or subclass of `<class 'int'>` in `super(<class 'int'>, str)` call"
# revealed: Unknown
reveal_type(super(int, str()))

# error: [invalid-super-argument] "`<class 'str'>` is not an instance or subclass of `<class 'int'>` in `super(<class 'int'>, <class 'str'>)` call"
# revealed: Unknown
reveal_type(super(int, str))

class A: ...
class B(A): ...

# error: [invalid-super-argument] "`A` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, A)` call"
# revealed: Unknown
reveal_type(super(B, A()))

# error: [invalid-super-argument] "`object` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, object)` call"
# revealed: Unknown
reveal_type(super(B, object()))

# error: [invalid-super-argument] "`<class 'A'>` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, <class 'A'>)` call"
# revealed: Unknown
reveal_type(super(B, A))

# error: [invalid-super-argument] "`<class 'object'>` is not an instance or subclass of `<class 'B'>` in `super(<class 'B'>, <class 'object'>)` call"
# revealed: Unknown
reveal_type(super(B, object))

super(object, object()).__class__

# Not all objects valid in a class's bases list are valid as the first argument to `super()`.
# For example, it's valid to inherit from `typing.ChainMap`, but it's not valid as the first argument to `super()`.
#
# error: [invalid-super-argument] "`<special-form 'typing.ChainMap'>` is not a valid class"
reveal_type(super(typing.ChainMap, collections.ChainMap()))  # revealed: Unknown

# Meanwhile, it's not valid to inherit from unsubscripted `typing.Generic`,
# but it *is* valid as the first argument to `super()`.
#
# revealed: <super: <special-form 'typing.Generic'>, <class 'SupportsInt'>>
reveal_type(super(typing.Generic, typing.SupportsInt))

def _(x: type[typing.Any], y: typing.Any):
    reveal_type(super(x, y))  # revealed: <super: Any, Any>
```

### Diagnostic when the invalid type is rendered very verbosely

<!-- snapshot-diagnostics -->

```py
def coinflip() -> bool:
    return False

def f():
    if coinflip():
        class A: ...
    else:
        class A: ...
    super(A, A())  # error: [invalid-super-argument]
```

### Instance Member Access via `super`

Accessing instance members through `super()` is not allowed.

```py
from __future__ import annotations

class A:
    def __init__(self, a: int):
        self.a = a

class B(A):
    def __init__(self, a: int):
        super().__init__(a)
        # error: [unresolved-attribute] "Object of type `<super: <class 'B'>, B>` has no attribute `a`"
        super().a

# error: [unresolved-attribute] "Object of type `<super: <class 'B'>, B>` has no attribute `a`"
super(B, B(42)).a
```

### Dunder Method Resolution

Dunder methods defined in the `owner` (from `super(pivot_class, owner)`) should not affect the super
object itself. In other words, `super` should not be treated as if it inherits attributes of the
`owner`.

```py
class A:
    def __getitem__(self, key: int) -> int:
        return 42

class B(A): ...

reveal_type(A()[0])  # revealed: int
reveal_type(super(B, B()).__getitem__)  # revealed: bound method B.__getitem__(key: int) -> int
# error: [not-subscriptable] "Cannot subscript object of type `<super: <class 'B'>, B>` with no `__getitem__` method"
super(B, B())[0]
```
