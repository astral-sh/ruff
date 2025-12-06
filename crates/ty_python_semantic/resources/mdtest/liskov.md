# The Liskov Substitution Principle

The Liskov Substitution Principle provides the basis for many of the assumptions a type checker
generally makes about types in Python:

> Subtype Requirement: Let ⁠`ϕ(x)`⁠ be a property provable about objects ⁠`x`⁠ of type `T`. Then
> ⁠`ϕ(y)`⁠ should be true for objects ⁠`y` of type `S` where `S` is a subtype of `T`.

In order for a type checker's assumptions to be sound, it is crucial for the type checker to enforce
the Liskov Substitution Principle on code that it checks. In practice, this usually manifests as
several checks for a type checker to perform when it checks a subclass `B` of a class `A`:

1. Read-only attributes should only ever be overridden covariantly: if a property `A.p` resolves to
    `int` when accessed, accessing `B.p` should either resolve to `int` or a subtype of `int`.
1. Method return types should only ever be overridden covariantly: if a method `A.f` returns `int`
    when called, calling `B.f` should also resolve to `int or a subtype of`int\`.
1. Method parameters should only ever be overridden contravariantly: if a method `A.f` can be called
    with an argument of type `bool`, then the method `B.f` must also be callable with type `bool`
    (though it is permitted for the override to also accept other types)
1. Mutable attributes should only ever be overridden invariantly: if a mutable attribute `A.attr`
    resolves to type `str`, it can only be overridden on a subclass with exactly the same type.

## Method return types

<!-- snapshot-diagnostics -->

```pyi
class Super:
    def method(self) -> int: ...

class Sub1(Super):
    def method(self) -> int: ...  # fine

class Sub2(Super):
    def method(self) -> bool: ...  # fine: `bool` is a subtype of `int`

class Sub3(Super):
    def method(self) -> object: ...  # error: [invalid-method-override]

class Sub4(Super):
    def method(self) -> str: ...  # error: [invalid-method-override]
```

## Method parameters

<!-- snapshot-diagnostics -->

```pyi
class Super:
    def method(self, x: int, /): ...

class Sub1(Super):
    def method(self, x: int, /): ...  # fine

class Sub2(Super):
    def method(self, x: object, /): ...  # fine: `method` still accepts any argument of type `int`

class Sub4(Super):
    def method(self, x: int | str, /): ...  # fine

class Sub5(Super):
    def method(self, x: int): ...  # fine: `x` can still be passed positionally

class Sub6(Super):
    # fine: `method()` can still be called with just a single argument
    def method(self, x: int, *args): ...

class Sub7(Super):
    def method(self, x: int, **kwargs): ...  # fine

class Sub8(Super):
    def method(self, x: int, *args, **kwargs): ...  # fine

class Sub9(Super):
    def method(self, x: int, extra_positional_arg=42, /): ... # fine

class Sub10(Super):
    def method(self, x: int, extra_pos_or_kw_arg=42): ...  # fine

class Sub11(Super):
    def method(self, x: int, *, extra_kw_only_arg=42): ...  # fine

class Sub12(Super):
    # Some calls permitted by the superclass are now no longer allowed
    # (the method can no longer be passed any arguments!)
    def method(self, /): ...  # error: [invalid-method-override]

class Sub13(Super):
    # Some calls permitted by the superclass are now no longer allowed
    # (the method can no longer be passed exactly one argument!)
    def method(self, x, y, /): ...  # error: [invalid-method-override]

class Sub14(Super):
    # Some calls permitted by the superclass are now no longer allowed
    # (x can no longer be passed positionally!)
    def method(self, /, *, x): ...  # error: [invalid-method-override]

class Sub15(Super):
    # Some calls permitted by the superclass are now no longer allowed
    # (x can no longer be passed any integer -- it now requires a bool!)
    def method(self, x: bool, /): ...  # error: [invalid-method-override]

class Super2:
    def method2(self, x): ...

class Sub16(Super2):
    def method2(self, x, /): ...  # error: [invalid-method-override]

class Sub17(Super2):
    def method2(self, *, x): ...  # error: [invalid-method-override]

class Super3:
    def method3(self, *, x): ...

class Sub18(Super3):
    def method3(self, x): ...  # fine: `x` can still be used as a keyword argument

class Sub19(Super3):
    def method3(self, x, /): ...  # error: [invalid-method-override]

class Super4:
    def method(self, *args: int, **kwargs: str): ...

class Sub20(Super4):
    def method(self, *args: object, **kwargs: object): ...  # fine

class Sub21(Super4):
    def method(self, *args): ...  # error: [invalid-method-override]

class Sub22(Super4):
    def method(self, **kwargs): ...  # error: [invalid-method-override]

class Sub23(Super4):
    def method(self, x, *args, y, **kwargs): ...  # error: [invalid-method-override]
```

## The entire class hierarchy is checked

If a child class's method definition is Liskov-compatible with the method definition on its parent
class, Liskov compatibility must also nonetheless be checked with respect to the method definition
on its grandparent class. This is because type checkers will treat the child class as a subtype of
the grandparent class just as much as they treat it as a subtype of the parent class, so
substitutability with respect to the grandparent class is just as important:

<!-- snapshot-diagnostics -->

`stub.pyi`:

```pyi
from typing import Any

class Grandparent:
    def method(self, x: int) -> None: ...

class Parent(Grandparent):
    def method(self, x: str) -> None: ...  # error: [invalid-method-override]

class Child(Parent):
    # compatible with the signature of `Parent.method`, but not with `Grandparent.method`:
    def method(self, x: str) -> None: ...  # error: [invalid-method-override]

class OtherChild(Parent):
    # compatible with the signature of `Grandparent.method`, but not with `Parent.method`:
    def method(self, x: int) -> None: ...  # error: [invalid-method-override]

class GradualParent(Grandparent):
    def method(self, x: Any) -> None: ...

class ThirdChild(GradualParent):
    # `GradualParent.method` is compatible with the signature of `Grandparent.method`,
    # and `ThirdChild.method` is compatible with the signature of `GradualParent.method`,
    # but `ThirdChild.method` is not compatible with the signature of `Grandparent.method`
    def method(self, x: str) -> None: ...  # error: [invalid-method-override]
```

`other_stub.pyi`:

```pyi
class A:
    def get(self, default): ...

class B(A):
    def get(self, default, /): ...  # error: [invalid-method-override]

get = 56

class C(B):
    # `get` appears in the symbol table of `C`,
    # but that doesn't confuse our diagnostic...
    foo = get

class D(C):
    # compatible with `C.get` and `B.get`, but not with `A.get`
    def get(self, my_default): ...  # error: [invalid-method-override]
```

## Non-generic methods on generic classes work as expected

```toml
[environment]
python-version = "3.12"
```

```pyi
class A[T]:
    def method(self, x: T) -> None: ...

class B[T](A[T]):
    def method(self, x: T) -> None: ...  # fine

class C(A[int]):
    def method(self, x: int) -> None: ...  # fine

class D[T](A[T]):
    def method(self, x: object) -> None: ...  # fine

class E(A[int]):
    def method(self, x: object) -> None: ...  # fine

class F[T](A[T]):
    # `str` is not necessarily a supertype of `T`!
    # error: [invalid-method-override]
    def method(self, x: str) -> None: ...

class G(A[int]):
    def method(self, x: bool) -> None: ...  # error: [invalid-method-override]
```

## Generic methods on non-generic classes work as expected

```toml
[environment]
python-version = "3.12"
```

```pyi
from typing import Never, Self

class A:
    def method[T](self, x: T) -> T: ...

class B(A):
    def method[T](self, x: T) -> T: ...  # fine

class C(A):
    def method(self, x: object) -> Never: ...  # fine

class D(A):
    # TODO: we should emit [invalid-method-override] here:
    # `A.method` accepts an argument of any type,
    # but `D.method` only accepts `int`s
    def method(self, x: int) -> int: ...

class A2:
    def method(self, x: int) -> int: ...

class B2(A2):
    # fine: although `B2.method()` will not always return an `int`,
    # an instance of `B2` can be substituted wherever an instance of `A2` is expected,
    # and it *will* always return an `int` if it is passed an `int`
    # (which is all that will be allowed if an instance of `A2` is expected)
    def method[T](self, x: T) -> T: ...

class C2(A2):
    def method[T: int](self, x: T) -> T: ...

class D2(A2):
    # The type variable is bound to a type disjoint from `int`,
    # so the method will not accept integers, and therefore this is an invalid override
    def method[T: str](self, x: T) -> T: ...  # error: [invalid-method-override]

class A3:
    def method(self) -> Self: ...

class B3(A3):
    def method(self) -> Self: ...  # fine

class C3(A3):
    # TODO: should this be allowed?
    # Mypy/pyright/pyrefly all allow it,
    # but conceptually it seems similar to `B4.method` below,
    # which mypy/pyrefly agree is a Liskov violation
    # (pyright disagrees as of 20/11/2025: https://github.com/microsoft/pyright/issues/11128)
    # when called on a subclass, `C3.method()` will not return an
    # instance of that subclass
    def method(self) -> C3: ...

class D3(A3):
    def method(self: Self) -> Self: ...  # fine

class E3(A3):
    def method(self: E3) -> Self: ...  # fine

class F3(A3):
    def method(self: A3) -> Self: ...  # fine

class G3(A3):
    def method(self: object) -> Self: ...  # fine

class H3(A3):
    # TODO: we should emit `invalid-method-override` here
    # (`A3.method()` can be called on any instance of `A3`,
    # but `H3.method()` can only be called on objects that are
    # instances of `str`)
    def method(self: str) -> Self: ...

class I3(A3):
    # TODO: we should emit `invalid-method-override` here
    # (`I3.method()` cannot be called with any inhabited type!)
    def method(self: Never) -> Self: ...

class A4:
    def method[T: int](self, x: T) -> T: ...

class B4(A4):
    # TODO: we should emit `invalid-method-override` here.
    # `A4.method` promises that if it is passed a `bool`, it will return a `bool`,
    # but this is not necessarily true for `B4.method`: if passed a `bool`,
    # it could return a non-`bool` `int`!
    def method(self, x: int) -> int: ...
```

## Generic methods on generic classes work as expected

```toml
[environment]
python-version = "3.12"
```

```pyi
from typing import Never

class A[T]:
    def method[S](self, x: T, y: S) -> S: ...

class B[T](A[T]):
    def method[S](self, x: T, y: S) -> S: ...  # fine

class C(A[int]):
    def method[S](self, x: int, y: S) -> S: ...  # fine

class D[T](A[T]):
    def method[S](self, x: object, y: S) -> S: ...  # fine

class E(A[int]):
    def method[S](self, x: object, y: S) -> S: ...  # fine

class F(A[int]):
    def method(self, x: object, y: object) -> Never: ...  # fine

class A2[T]:
    def method(self, x: T, y: int) -> int: ...

class B2[T](A2[T]):
    def method[S](self, x: T, y: S) -> S: ...  # fine
```

## Fully qualified names are used in diagnostics where appropriate

<!-- snapshot-diagnostics -->

`a.pyi`:

```pyi
class A:
    def foo(self, x): ...
```

`b.pyi`:

```pyi
import a

class A(a.A):
    def foo(self, y): ...  # error: [invalid-method-override]
```

## Excluded methods

Certain special constructor methods are excluded from Liskov checks. None of the following classes
cause us to emit any errors, therefore:

```toml
# This is so that the dataclasses machinery will generate `__replace__` methods for us
# (the synthesized `__replace__` methods should not be reported as invalid overrides!)
[environment]
python-version = "3.13"
```

```pyi
from dataclasses import dataclass
from typing_extensions import Self

class Grandparent: ...
class Parent(Grandparent):
    def __new__(cls, x: int) -> Self: ...
    def __init__(self, x: int) -> None: ...

class Child(Parent):
    def __new__(cls, x: str, y: str) -> Self: ...
    def __init__(self, x: str, y: str) -> Self: ...

@dataclass(init=False)
class DataSuper:
    x: int

    def __post_init__(self, x: int) -> None:
        self.x = x

@dataclass(init=False)
class DataSub(DataSuper):
    y: str

    def __post_init__(self, x: int, y: str) -> None:
        self.y = y
        super().__post_init__(x)
```

## Edge case: function defined in another module and then assigned in a class body

<!-- snapshot-diagnostics -->

`foo.pyi`:

```pyi
def x(self, y: str): ...
```

`bar.pyi`:

```pyi
import foo

class A:
    def x(self, y: int): ...

class B(A):
    x = foo.x  # error: [invalid-method-override]

class C:
    x = foo.x

class D(C):
    def x(self, y: int): ...  # error: [invalid-method-override]
```

## Bad override of `__eq__`

<!-- snapshot-diagnostics -->

```py
class Bad:
    x: int
    def __eq__(self, other: "Bad") -> bool:  # error: [invalid-method-override]
        return self.x == other.x
```

## Synthesized methods

`NamedTuple` classes and dataclasses both have methods generated at runtime that do not have
source-code definitions. There are several scenarios to consider here:

1. A synthesized method on a superclass is overridden by a "normal" (not synthesized) method on a
    subclass
1. A "normal" method on a superclass is overridden by a synthesized method on a subclass
1. A synthesized method on a superclass is overridden by a synthesized method on a subclass

<!-- snapshot-diagnostics -->

```pyi
from dataclasses import dataclass
from typing import NamedTuple

@dataclass(order=True)
class Foo:
    x: int

class Bar(Foo):
    def __lt__(self, other: Bar) -> bool: ...  # error: [invalid-method-override]

# TODO: specifying `order=True` on the subclass means that a `__lt__` method is
# generated that is incompatible with the generated `__lt__` method on the superclass.
# We could consider detecting this and emitting a diagnostic, though maybe it shouldn't
# be `invalid-method-override` since we'd emit it on the class definition rather than
# on any method definition. Note also that no other type checker complains about this
# as of 2025-11-21.
@dataclass(order=True)
class Bar2(Foo):
    y: str

# TODO: Although this class does not override any methods of `Foo`, the design of the
# `order=True` stdlib dataclasses feature itself arguably violates the Liskov Substitution
# Principle! Instances of `Bar3` cannot be substituted wherever an instance of `Foo` is
# expected, because the generated `__lt__` method on `Foo` raises an error unless the r.h.s.
# and `l.h.s.` have exactly the same `__class__` (it does not permit instances of `Foo` to
# be compared with instances of subclasses of `Foo`).
#
# Many users would probably like their type checkers to alert them to cases where instances
# of subclasses cannot be substituted for instances of superclasses, as this violates many
# assumptions a type checker will make and makes it likely that a type checker will fail to
# catch type errors elsewhere in the user's code. We could therefore consider treating all
# `order=True` dataclasses as implicitly `@final` in order to enforce soundness. However,
# this probably shouldn't be reported with the same error code as Liskov violations, since
# the error does not stem from any method signatures written by the user. The example is
# only included here for completeness.
#
# Note that no other type checker catches this error as of 2025-11-21.
class Bar3(Foo): ...

class Eggs:
    def __lt__(self, other: Eggs) -> bool: ...

# TODO: the generated `Ham.__lt__` method here incompatibly overrides `Eggs.__lt__`.
# We could consider emitting a diagnostic here. As of 2025-11-21, mypy reports a
# diagnostic here but pyright and pyrefly do not.
@dataclass(order=True)
class Ham(Eggs):
    x: int

class Baz(NamedTuple):
    x: int

class Spam(Baz):
    def _asdict(self) -> tuple[int, ...]: ...  # error: [invalid-method-override]
```

## Staticmethods and classmethods

Methods decorated with `@staticmethod` or `@classmethod` are checked in much the same way as other
methods.

<!-- snapshot-diagnostics -->

```pyi
class Parent:
    def instance_method(self, x: int) -> int: ...
    @classmethod
    def class_method(cls, x: int) -> int: ...
    @staticmethod
    def static_method(x: int) -> int: ...

class BadChild1(Parent):
    @staticmethod
    def instance_method(self, x: int) -> int: ...  # error: [invalid-method-override]
    # TODO: we should emit `invalid-method-override` here.
    # Although the method has the same signature as `Parent.class_method`
    # when accessed on instances, it does not have the same signature as
    # `Parent.class_method` when accessed on the class object itself
    def class_method(cls, x: int) -> int: ...
    def static_method(x: int) -> int: ...  # error: [invalid-method-override]

class BadChild2(Parent):
    # TODO: we should emit `invalid-method-override` here.
    # Although the method has the same signature as `Parent.class_method`
    # when accessed on instances, it does not have the same signature as
    # `Parent.class_method` when accessed on the class object itself.
    #
    # Note that whereas `BadChild1.class_method` is reported as a Liskov violation by
    # mypy, pyright and pyrefly, pyright is the only one of those three to report a
    # Liskov violation on this method as of 2025-11-23.
    @classmethod
    def instance_method(self, x: int) -> int: ...
    @staticmethod
    def class_method(cls, x: int) -> int: ...  # error: [invalid-method-override]
    @classmethod
    def static_method(x: int) -> int: ...  # error: [invalid-method-override]

class BadChild3(Parent):
    @classmethod
    def class_method(cls, x: bool) -> object: ...  # error: [invalid-method-override]
    @staticmethod
    def static_method(x: bool) -> object: ...  # error: [invalid-method-override]

class GoodChild1(Parent):
    @classmethod
    def class_method(cls, x: int) -> int: ...
    @staticmethod
    def static_method(x: int) -> int: ...

class GoodChild2(Parent):
    @classmethod
    def class_method(cls, x: object) -> bool: ...
    @staticmethod
    def static_method(x: object) -> bool: ...
```

## Definitely bound members with no reachable definitions(!)

We don't emit a Liskov-violation diagnostic here, but if you're writing code like this, you probably
have bigger problems:

```py
from __future__ import annotations

class MaybeEqWhile:
    while ...:
        def __eq__(self, other: MaybeEqWhile) -> bool:
            return True
```
