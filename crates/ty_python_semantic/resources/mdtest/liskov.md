# The Liskov Substitution Principle

The Liskov Substitution Principle provides the basis for many of the assumptions a type checker
generally makes about types in Python:

> Subtype Requirement: Let ⁠`ϕ(x)`⁠ be a property provable about objects ⁠`x`⁠ of type `T`. Then
> ⁠`ϕ(y)`⁠ should be true for objects ⁠`y` of type `S` where `S` is a subtype of `T`.

In order for a type checker's assumptions to be sound, it is crucial for the type checker to enforce
the Liskov Substitution Principle on code that it checks. In practice, this usually manifests as
three checks for a type checker to perform when it checks a subclass `B` of a class `A`:

1. Read-only attributes should only ever be overridden covariantly: if a property `A.p` resolves to
    `int` when accessed, accessing `B.p` should either resolve to `int` or a subtype of `int`.
1. Method return types should only ever be overidden covariantly: if a method `A.f` returns `int`
    when called, calling `B.f` should also resolve to `int or a subtype of`int\`.
1. Method parameters should only ever be overridden contravariantly: if a method `A.f` can be called
    with an argument of type `bool`, then the method `B.f` must also be callable with type `bool`
    (though it is permitted for the override to also accept other types)
1. Mutable attributes should only ever be overridden invariantly: if a mutable attribute `A.attr`
    resolves to type `str`, it can only be overidden on a subclass with exactly the same type.

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
    def method(self, x: object, /): ...  # fine: `method` still accepts any argument of type `bool`

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
    # (the method can no longer be passed with exactly one argument!)
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
