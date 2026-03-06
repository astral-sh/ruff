## Custom `__call__` on metaclass

When a metaclass defines a custom `__call__` method, it controls what happens when the class is
called. If the metaclass `__call__` returns an "instance type" (subtype of the class being
constructed), then the class' `__new__` and `__init__` are checked as usual (see `classes.md`). But
if the metaclass `__call__` returns a non-instance type, then `__new__` and `__init__` are skipped
and the return type of `__call__` is used directly.

### Metaclass `__call__` returning non-instance type

```py
class Meta(type):
    def __call__(cls, x: int, y: str) -> str:
        return y

class Foo(metaclass=Meta): ...

reveal_type(Foo(1, "hello"))  # revealed: str

a: str = Foo(1, "hello")  # OK
```

### Metaclass `__call__` takes precedence over `__init__` and `__new__`

```py
class Meta(type):
    def __call__(cls) -> str:
        return "hello"

class Foo(metaclass=Meta):
    def __new__(cls, x: int) -> "Foo":
        return object.__new__(cls)

    def __init__(self, x: int, y: int) -> None:
        pass

# The metaclass __call__ takes precedence, so no arguments are needed
# and the return type is str, not Foo.
reveal_type(Foo())  # revealed: str
```

### Metaclass `__call__` with wrong arguments

```py
class Meta(type):
    def __call__(cls, x: int) -> int:
        return x

class Foo(metaclass=Meta): ...

# error: [invalid-argument-type]
reveal_type(Foo("wrong"))  # revealed: int
# error: [missing-argument]
reveal_type(Foo())  # revealed: int
# error: [too-many-positional-arguments]
reveal_type(Foo(1, 2))  # revealed: int
```

### Metaclass `__call__` with TypeVar return type

When the metaclass `__call__` returns a TypeVar bound to the class type, it's essentially a
pass-through to the normal constructor machinery. In this case, we should still check the `__new__`
and `__init__` signatures.

```py
from typing import TypeVar

T = TypeVar("T")

class Meta(type):
    def __call__(cls: type[T], *args, **kwargs) -> T:
        return object.__new__(cls)

class Foo(metaclass=Meta):
    def __init__(self, x: int) -> None:
        pass

# The metaclass __call__ returns T (bound to Foo), so we check __init__ parameters.
Foo()  # error: [missing-argument]
reveal_type(Foo(1))  # revealed: Foo
```

### Metaclass `__call__` with no return type annotation

When the metaclass `__call__` has no return type annotation (returns `Unknown`), we should still
check the `__new__` and `__init__` signatures, and infer the instance return type.

```py
class Meta(type):
    def __call__(cls, *args, **kwargs):
        return object.__new__(cls)

class Foo(metaclass=Meta):
    def __init__(self, x: int) -> None:
        pass

# No return type annotation means we fall through to check __init__ parameters.
Foo()  # error: [missing-argument]
reveal_type(Foo(1))  # revealed: Foo
```

### Metaclass `__call__` with specific parameters

When the metaclass `__call__` has specific parameters (not just `*args, **kwargs`), we validate them
even when the return type is an instance type. Here both `__new__` and `__init__` accept anything,
so the errors must come from the metaclass `__call__`.

```py
from typing import Any, TypeVar

T = TypeVar("T")

class Meta(type):
    def __call__(cls: type[T], x: int) -> T:
        return object.__new__(cls)

class Foo(metaclass=Meta):
    def __new__(cls, *args: Any, **kwargs: Any) -> "Foo":
        return object.__new__(cls)

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        pass

# The metaclass `__call__` requires exactly one `int` argument.
# error: [invalid-argument-type]
reveal_type(Foo("wrong"))  # revealed: Foo
# error: [missing-argument]
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments]
reveal_type(Foo(1, 2))  # revealed: Foo
reveal_type(Foo(1))  # revealed: Foo
```

### Metaclass `__call__` returning the class instance type

When the metaclass `__call__` returns the constructed class type (or a subclass), it's not
overriding normal construction. Per the spec, `__new__`/`__init__` should still be evaluated.

```py
class Meta(type):
    def __call__(cls, *args, **kwargs) -> "Foo":
        return super().__call__(*args, **kwargs)

class Foo(metaclass=Meta):
    def __init__(self, x: int) -> None:
        pass

# The metaclass __call__ returns Foo, so we fall through to check __init__.
Foo()  # error: [missing-argument]
Foo("wrong")  # error: [invalid-argument-type]
reveal_type(Foo(1))  # revealed: Foo
```

### Metaclass `__call__` returning a specific class affects subclasses

When a metaclass `__call__` returns a specific class (e.g., `-> Foo`), this is an instance type for
`Foo` itself, so `__init__` is checked. But for a subclass `Bar(Foo)`, the return type `Foo` is NOT
an instance of `Bar`, so the metaclass `__call__` is used directly and `Bar.__init__` is skipped.

```py
from typing import Any

class Meta(type):
    def __call__(cls, *args: Any, **kwargs: Any) -> "Foo":
        return super().__call__(*args, **kwargs)

class Foo(metaclass=Meta):
    def __init__(self, x: int) -> None:
        pass

class Bar(Foo):
    def __init__(self, y: str) -> None:
        pass

# For Foo: return type `Foo` IS an instance of `Foo`, so `__init__` is checked.
Foo()  # error: [missing-argument]
reveal_type(Foo(1))  # revealed: Foo

# For Bar: return type `Foo` is NOT an instance of `Bar`, so `__init__` is
# skipped and the metaclass `__call__` (which accepts `*args, **kwargs`) is
# used directly.
reveal_type(Bar())  # revealed: Foo
reveal_type(Bar("hello"))  # revealed: Foo
```

### Metaclass `__call__` returning `Any`

When a metaclass `__call__` returns `Any`, the spec says to assume that the return type is not an
instance of the class being constructed, so we use the metaclass `__call__` signature directly and
skip `__new__`/`__init__` validation. It's a bit odd to have different behavior for `-> Any` than
for no annotation, but that's what the spec says, and for now we follow it.

```py
from typing import Any

class Meta(type):
    def __call__(cls, *args: Any, **kwargs: Any) -> Any:
        return super().__call__(*args, **kwargs)

class Foo(metaclass=Meta):
    def __init__(self, x: int) -> None:
        pass

# The metaclass `__call__` accepts `(*args, **kwargs)` and returns `Any`,
# so we use that directly, skipping `__init__` validation.
reveal_type(Foo())  # revealed: Any
reveal_type(Foo("wrong"))  # revealed: Any
```

### Metaclass `__call__` returning `Never`

When metaclass `__call__` returns `Never`, construction is terminal. We use metaclass `__call__`
directly and skip `__new__` and `__init__`.

```py
from typing_extensions import Never

class Meta(type):
    def __call__(cls) -> Never:
        raise NotImplementedError

class C(metaclass=Meta):
    def __new__(cls, x: int) -> "C":
        return object.__new__(cls)

    def __init__(self, x: int) -> None:
        pass

# `__new__` and `__init__` are skipped because metaclass `__call__` never returns.
reveal_type(C())  # revealed: Never
```

### Overloaded metaclass `__call__` with mixed return types

When a metaclass `__call__` is overloaded and some overloads return the class instance type while
others return a different type, non-instance-returning overloads use the metaclass `__call__`
directly, while instance-returning overloads are replaced by `__init__` validation.

```py
from typing import Any, overload
from typing_extensions import Literal

class Meta(type):
    @overload
    def __call__(cls, x: int) -> int: ...
    @overload
    def __call__(cls, x: str) -> "Foo": ...
    def __call__(cls, x: int | str) -> Any:
        return super().__call__(x)

class Foo(metaclass=Meta):
    def __init__(self, x: int) -> None:
        pass

# The `int` overload from the metaclass `__call__` is selected; its return type
# is not an instance of `Foo`, so it is used directly.
reveal_type(Foo(1))  # revealed: int

# The `str -> Foo` metaclass overload matches and returns an instance, so `__init__`
# is also validated. `__init__` expects `x: int`, but got `str`.
Foo("hello")  # error: [invalid-argument-type]

# No overload matches.
Foo()  # error: [no-matching-overload]
```

### Mixed metaclass `__call__` overloads should not become declaration-order dependent

Reversing the declaration order of the same mixed overload set should not change the result when
overload resolution falls back to `Unknown`.

```py
from typing import Any, TypeVar, overload
from missing import Unknown  # type: ignore

T = TypeVar("T")

class ReverseMeta(type):
    @overload
    def __call__(cls: type[T], x: str) -> str: ...
    @overload
    def __call__(cls: type[T], x: int) -> T: ...
    def __call__(cls, x: int | str) -> object:
        return super().__call__()

class ReverseMetaTarget(metaclass=ReverseMeta):
    def __init__(self) -> None: ...

def _(a: Any, u: Unknown):
    reveal_type(ReverseMetaTarget(a))  # revealed: ReverseMetaTarget
    reveal_type(ReverseMetaTarget(u))  # revealed: ReverseMetaTarget
```

### Overloaded metaclass `__call__` preserving strict-subclass return

```py
from typing import Any, overload

class Meta(type):
    @overload
    def __call__(cls, x: int) -> int: ...
    @overload
    def __call__(cls, x: str) -> "Child": ...
    def __call__(cls, x: int | str) -> Any:
        return super().__call__(x)

class Parent(metaclass=Meta):
    def __init__(self, x: str) -> None:
        pass

class Child(Parent): ...

reveal_type(Parent(1))  # revealed: int
reveal_type(Parent("hello"))  # revealed: Child
```

### Overloaded metaclass `__call__` returning only non-instance types

When all overloads of a metaclass `__call__` return non-instance types, the metaclass fully
overrides `type.__call__` and `__init__` is not checked.

```py
from typing import Any, overload

class Meta(type):
    @overload
    def __call__(cls, x: int) -> int: ...
    @overload
    def __call__(cls, x: str) -> str: ...
    def __call__(cls, x: int | str) -> Any:
        return x

class Bar(metaclass=Meta):
    def __init__(self, x: int, y: int) -> None:
        pass

# `__init__` is not checked: it requires two `int` args, but we only pass one.
# No error is raised because the metaclass `__call__` controls construction.
reveal_type(Bar(1))  # revealed: int
reveal_type(Bar("hello"))  # revealed: str
```

### Invalid overloaded non-instance metaclass `__call__` should not invent an instance return

If no overload matches, we should still report `Unknown` rather than falling back to the class
instance type.

```py
from typing import overload

class OnlyNonInstanceMeta(type):
    @overload
    def __call__(cls, x: int) -> int: ...
    @overload
    def __call__(cls, x: str) -> str: ...
    def __call__(cls, x: int | str) -> object:
        raise NotImplementedError

class OnlyNonInstanceMetaTarget(metaclass=OnlyNonInstanceMeta):
    pass

# error: [no-matching-overload]
reveal_type(OnlyNonInstanceMetaTarget(1.2))  # revealed: Unknown
```

### Overloaded metaclass `__call__` with non-class return forms

When all overloads return non-instance types that aren't simple class instances (e.g., `Callable`),
`__init__` should still be skipped.

```py
from typing import Any, Callable, overload

class Meta(type):
    @overload
    def __call__(cls, x: int) -> Callable[[], int]: ...
    @overload
    def __call__(cls, x: str) -> Callable[[], str]: ...
    def __call__(cls, x: int | str) -> Any:
        return lambda: x

class Baz(metaclass=Meta):
    def __init__(self, x: int, y: int) -> None:
        pass

# `__init__` is not checked: it requires two `int` args, but we only pass one.
# No error is raised because the metaclass `__call__` controls construction.
reveal_type(Baz(1))  # revealed: () -> int
reveal_type(Baz("hello"))  # revealed: () -> str
```

### If metaclass `__call__` fails, `__new__` is irrelevant

```py
class Meta(type):
    def __call__(cls, x: str) -> "C":
        raise NotImplementedError

class C(metaclass=Meta):
    def __new__(cls, x: bytes) -> int:
        return 1

# error: [invalid-argument-type]
reveal_type(C(b"hello"))  # revealed: C
```

### Metaclass `__call__` is not a simple method

```py
class MetaCall:
    def __call__(self) -> int:
        return 1

class Meta(type):
    __call__: MetaCall = MetaCall()

class C(metaclass=Meta): ...

reveal_type(C())  # revealed: int
```

### Invalid overloaded downstream `__new__` should preserve `Unknown`

If metaclass `__call__` forwards to normal construction by returning the constructed instance type,
and the downstream overloaded `__new__` doesn't match, we should preserve the downstream `Unknown`
return rather than falling back to the class instance type.

```py
from typing import TypeVar, overload

T = TypeVar("T")

class Meta(type):
    def __call__(cls: type[T], x: object) -> T:
        raise NotImplementedError

class D(metaclass=Meta):
    @overload
    def __new__(cls, x: int) -> int: ...
    @overload
    def __new__(cls, x: str) -> str: ...
    def __new__(cls, x: object) -> object:
        raise NotImplementedError

# error: [no-matching-overload]
reveal_type(D(1.2))  # revealed: Unknown
```

### Mixed `__new__` and mixed metaclass `__call__`

If both metaclass `__call__` and `__new__` are mixed (some overloads instance-returning and some
non-instance), the fallback chain works as expected: `__new__` is only considered if metaclass
`__call__` is instance-returning, and `__init__` is only considered if both `__call__` and `__new__`
are instance-returning.

```py
from __future__ import annotations
from typing import Any, Literal, overload

class A: ...
class B: ...
class C: ...
class D: ...

class Meta(type):
    @overload
    def __call__(cls, x: A) -> A: ...
    @overload
    def __call__(cls, x: B) -> Test: ...
    @overload
    def __call__(cls, x: C) -> Test: ...
    @overload
    def __call__(cls, x: str) -> Test: ...
    def __call__(cls, x: A | B | C | str) -> A | Test:
        raise NotImplementedError()

class Test(metaclass=Meta):
    @overload
    def __new__(cls, x: B) -> B: ...
    @overload
    def __new__(cls, x: D) -> D: ...
    @overload
    def __new__(cls, x: str) -> Test: ...
    def __new__(cls, x: B | D | str) -> B | D | Test:
        raise NotImplementedError()

    def __init__(self, x: Literal["ok"]) -> None:
        pass

# `A` matches the first metaclass overload, which returns `A`, bypassing `__new__` and `__init__`
# since `A` is not a subtype of `Test`.
reveal_type(Test(A()))  # revealed: A

# `B` returns `Test` from metaclass `__call__` and returns `B` from `__new__`, bypassing `__init__`
# since `B` is not a subtype of `Test`.
reveal_type(Test(B()))  # revealed: B

# `C` returns `Test` from metaclass `__call__` and fails the call to `__new__`.
# error: [no-matching-overload]
reveal_type(Test(C()))  # revealed: Test

# `D` fails metaclass `__call__`, so never reaches `__new__` or `__init__`.
# error: [no-matching-overload]
reveal_type(Test(D()))  # revealed: Test

# `str` returns `Test` from both `__call__` and `__new__`, but `__init__` rejects `Literal["bad"]`.
# error: [invalid-argument-type]
reveal_type(Test("bad"))  # revealed: Test

# `Literal["ok"]` returns `Test` from both `__call__` and `__new__`, and is accepted by `__init__`.
reveal_type(Test("ok"))  # revealed: Test
```

## Default

```py
class M(type): ...

reveal_type(M.__class__)  # revealed: <class 'type'>
```

## `object`

```py
reveal_type(object.__class__)  # revealed: <class 'type'>
```

## `type`

```py
reveal_type(type.__class__)  # revealed: <class 'type'>
```

## Basic

```py
class M(type): ...
class B(metaclass=M): ...

reveal_type(B.__class__)  # revealed: <class 'M'>
```

## Invalid metaclass

A class which doesn't inherit `type` (and/or doesn't implement a custom `__new__` accepting the same
arguments as `type.__new__`) isn't a valid metaclass.

```py
class M: ...
class A(metaclass=M): ...

# TODO: emit a diagnostic for the invalid metaclass
reveal_type(A.__class__)  # revealed: <class 'M'>
```

## Linear inheritance

If a class is a subclass of a class with a custom metaclass, then the subclass will also have that
metaclass.

```py
class M(type): ...
class A(metaclass=M): ...
class B(A): ...

reveal_type(B.__class__)  # revealed: <class 'M'>
```

## Linear inheritance with PEP 695 generic class

The same is true if the base with the metaclass is a generic class.

```toml
[environment]
python-version = "3.13"
```

```py
class M(type): ...
class A[T](metaclass=M): ...
class B(A): ...
class C(A[int]): ...

reveal_type(B.__class__)  # revealed: <class 'M'>
reveal_type(C.__class__)  # revealed: <class 'M'>
```

## Conflict (1)

The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its
bases. ("Strict subclass" is a synonym for "proper subclass"; a non-strict subclass can be a
subclass or the class itself.)

```py
class M1(type): ...
class M2(type): ...
class A(metaclass=M1): ...
class B(metaclass=M2): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`C`) must be a subclass of the metaclasses of all its bases, but `M1` (metaclass of base class `A`) and `M2` (metaclass of base class `B`) have no subclass relationship"
class C(A, B): ...

reveal_type(C.__class__)  # revealed: type[Unknown]
```

## Conflict (2)

The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its
bases. ("Strict subclass" is a synonym for "proper subclass"; a non-strict subclass can be a
subclass or the class itself.)

```py
class M1(type): ...
class M2(type): ...
class A(metaclass=M1): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`B`) must be a subclass of the metaclasses of all its bases, but `M2` (metaclass of `B`) and `M1` (metaclass of base class `A`) have no subclass relationship"
class B(A, metaclass=M2): ...

reveal_type(B.__class__)  # revealed: type[Unknown]
```

## Common metaclass

A class has two explicit bases, both of which have the same metaclass.

```py
class M(type): ...
class A(metaclass=M): ...
class B(metaclass=M): ...
class C(A, B): ...

reveal_type(C.__class__)  # revealed: <class 'M'>
```

## Metaclass metaclass

A class has an explicit base with a custom metaclass. That metaclass itself has a custom metaclass.

```py
class M1(type): ...
class M2(type, metaclass=M1): ...
class M3(M2): ...
class A(metaclass=M3): ...
class B(A): ...

reveal_type(A.__class__)  # revealed: <class 'M3'>
```

## Diamond inheritance

```py
class M(type): ...
class M1(M): ...
class M2(M): ...
class M12(M1, M2): ...
class A(metaclass=M1): ...
class B(metaclass=M2): ...
class C(metaclass=M12): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`D`) must be a subclass of the metaclasses of all its bases, but `M1` (metaclass of base class `A`) and `M2` (metaclass of base class `B`) have no subclass relationship"
class D(A, B, C): ...

reveal_type(D.__class__)  # revealed: type[Unknown]
```

## Unknown

```py
from nonexistent_module import UnknownClass  # error: [unresolved-import]

class C(UnknownClass): ...

# TODO: should be `type[type] & Unknown`
reveal_type(C.__class__)  # revealed: <class 'type'>

class M(type): ...
class A(metaclass=M): ...
class B(A, UnknownClass): ...

# TODO: should be `type[M] & Unknown`
reveal_type(B.__class__)  # revealed: <class 'M'>
```

## Duplicate

```py
class M(type): ...
class A(metaclass=M): ...
class B(A, A): ...  # error: [duplicate-base] "Duplicate base class `A`"

reveal_type(B.__class__)  # revealed: <class 'M'>
```

## Non-class

When a class has an explicit `metaclass` that is not a class, but is a callable that accepts
`type.__new__` arguments, we should return the meta-type of its return type.

```py
def f(*args, **kwargs) -> int:
    return 1

class A(metaclass=f): ...

# TODO: Should be `int`
reveal_type(A)  # revealed: <class 'A'>
reveal_type(A.__class__)  # revealed: type[int]

def _(n: int):
    # error: [invalid-metaclass]
    class B(metaclass=n): ...
    # TODO: Should be `Unknown`
    reveal_type(B)  # revealed: <class 'B'>
    reveal_type(B.__class__)  # revealed: type[Unknown]

def _(flag: bool):
    m = f if flag else 42

    # error: [invalid-metaclass]
    class C(metaclass=m): ...
    # TODO: Should be `int | Unknown`
    reveal_type(C)  # revealed: <class 'C'>
    reveal_type(C.__class__)  # revealed: type[Unknown]

class SignatureMismatch: ...

# TODO: Emit a diagnostic
class D(metaclass=SignatureMismatch): ...

# TODO: Should be `Unknown`
reveal_type(D)  # revealed: <class 'D'>
# TODO: Should be `type[Unknown]`
reveal_type(D.__class__)  # revealed: <class 'SignatureMismatch'>
```

## Cyclic

Retrieving the metaclass of a cyclically defined class should not cause an infinite loop.

```pyi
class A(B): ...  # error: [cyclic-class-definition]
class B(C): ...  # error: [cyclic-class-definition]
class C(A): ...  # error: [cyclic-class-definition]

reveal_type(A.__class__)  # revealed: type[Unknown]
```

## PEP 695 generic

```toml
[environment]
python-version = "3.12"
```

```py
class M(type): ...
class A[T: str](metaclass=M): ...

reveal_type(A.__class__)  # revealed: <class 'M'>
```

## Generic metaclass

### Fully specialized

A generic metaclass fully specialized with concrete types is fine:

```toml
[environment]
python-version = "3.13"
```

```py
class Foo[T](type):
    x: T

class Bar(metaclass=Foo[int]): ...

reveal_type(Bar.__class__)  # revealed: <class 'Foo[int]'>
```

### Parameterized by type variables (legacy)

A generic metaclass parameterized by type variables is not supported:

```py
from typing import TypeVar, Generic

T = TypeVar("T")

class GenericMeta(type, Generic[T]): ...

# error: [invalid-metaclass] "Generic metaclasses are not supported"
class GenericMetaInstance(metaclass=GenericMeta[T]): ...
```

### Parameterized by type variables (PEP 695)

The same applies using PEP 695 syntax:

```toml
[environment]
python-version = "3.13"
```

```py
class Foo[T](type):
    x: T

# error: [invalid-metaclass]
class Bar[T](metaclass=Foo[T]): ...
```

## Metaclasses of metaclasses

```py
class Foo(type): ...
class Bar(type, metaclass=Foo): ...
class Baz(type, metaclass=Bar): ...
class Spam(metaclass=Baz): ...

reveal_type(Spam.__class__)  # revealed: <class 'Baz'>
reveal_type(Spam.__class__.__class__)  # revealed: <class 'Bar'>
reveal_type(Spam.__class__.__class__.__class__)  # revealed: <class 'Foo'>

def test(x: Spam):
    reveal_type(x.__class__)  # revealed: type[Spam]
    reveal_type(x.__class__.__class__)  # revealed: type[Baz]
    reveal_type(x.__class__.__class__.__class__)  # revealed: type[Bar]
    reveal_type(x.__class__.__class__.__class__.__class__)  # revealed: type[Foo]
    reveal_type(x.__class__.__class__.__class__.__class__.__class__)  # revealed: type[type]

    # revealed: type[type]
    reveal_type(x.__class__.__class__.__class__.__class__.__class__.__class__.__class__.__class__)
```
