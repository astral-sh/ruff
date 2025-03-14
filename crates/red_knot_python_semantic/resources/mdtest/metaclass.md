## Default

```py
class M(type): ...

reveal_type(M.__class__)  # revealed: Literal[type]
```

## `object`

```py
reveal_type(object.__class__)  # revealed: Literal[type]
```

## `type`

```py
reveal_type(type.__class__)  # revealed: Literal[type]
```

## Basic

```py
class M(type): ...
class B(metaclass=M): ...

reveal_type(B.__class__)  # revealed: Literal[M]
```

## Invalid metaclass

A class which doesn't inherit `type` (and/or doesn't implement a custom `__new__` accepting the same
arguments as `type.__new__`) isn't a valid metaclass.

```py
class M: ...
class A(metaclass=M): ...

# TODO: emit a diagnostic for the invalid metaclass
reveal_type(A.__class__)  # revealed: Literal[M]
```

## Linear inheritance

If a class is a subclass of a class with a custom metaclass, then the subclass will also have that
metaclass.

```py
class M(type): ...
class A(metaclass=M): ...
class B(A): ...

reveal_type(B.__class__)  # revealed: Literal[M]
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

reveal_type(C.__class__)  # revealed: Literal[M]
```

## Metaclass metaclass

A class has an explicit base with a custom metaclass. That metaclass itself has a custom metaclass.

```py
class M1(type): ...
class M2(type, metaclass=M1): ...
class M3(M2): ...
class A(metaclass=M3): ...
class B(A): ...

reveal_type(A.__class__)  # revealed: Literal[M3]
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
reveal_type(C.__class__)  # revealed: Literal[type]

class M(type): ...
class A(metaclass=M): ...
class B(A, UnknownClass): ...

# TODO: should be `type[M] & Unknown`
reveal_type(B.__class__)  # revealed: Literal[M]
```

## Duplicate

```py
class M(type): ...
class A(metaclass=M): ...
class B(A, A): ...  # error: [duplicate-base] "Duplicate base class `A`"

reveal_type(B.__class__)  # revealed: Literal[M]
```

## Non-class

When a class has an explicit `metaclass` that is not a class, but is a callable that accepts
`type.__new__` arguments, we should return the meta-type of its return type.

```py
def f(*args, **kwargs) -> int:
    return 1

class A(metaclass=f): ...

# TODO: Should be `int`
reveal_type(A)  # revealed: Literal[A]
reveal_type(A.__class__)  # revealed: type[int]

def _(n: int):
    # error: [invalid-metaclass]
    class B(metaclass=n): ...
    # TODO: Should be `Unknown`
    reveal_type(B)  # revealed: Literal[B]
    reveal_type(B.__class__)  # revealed: type[Unknown]

def _(flag: bool):
    m = f if flag else 42

    # error: [invalid-metaclass]
    class C(metaclass=m): ...
    # TODO: Should be `int | Unknown`
    reveal_type(C)  # revealed: Literal[C]
    reveal_type(C.__class__)  # revealed: type[Unknown]

class SignatureMismatch: ...

# TODO: Emit a diagnostic
class D(metaclass=SignatureMismatch): ...

# TODO: Should be `Unknown`
reveal_type(D)  # revealed: Literal[D]
# TODO: Should be `type[Unknown]`
reveal_type(D.__class__)  # revealed: Literal[SignatureMismatch]
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

```py
class M(type): ...
class A[T: str](metaclass=M): ...

reveal_type(A.__class__)  # revealed: Literal[M]
```

## Metaclasses of metaclasses

```py
class Foo(type): ...
class Bar(type, metaclass=Foo): ...
class Baz(type, metaclass=Bar): ...
class Spam(metaclass=Baz): ...

reveal_type(Spam.__class__)  # revealed: Literal[Baz]
reveal_type(Spam.__class__.__class__)  # revealed: Literal[Bar]
reveal_type(Spam.__class__.__class__.__class__)  # revealed: Literal[Foo]

def test(x: Spam):
    reveal_type(x.__class__)  # revealed: type[Spam]
    reveal_type(x.__class__.__class__)  # revealed: type[Baz]
    reveal_type(x.__class__.__class__.__class__)  # revealed: type[Bar]
    reveal_type(x.__class__.__class__.__class__.__class__)  # revealed: type[Foo]
    reveal_type(x.__class__.__class__.__class__.__class__.__class__)  # revealed: type[type]

    # revealed: type[type]
    reveal_type(x.__class__.__class__.__class__.__class__.__class__.__class__.__class__.__class__)
```
