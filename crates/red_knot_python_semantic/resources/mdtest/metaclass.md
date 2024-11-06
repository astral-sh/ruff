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
class A(type): ...
class B(metaclass=A): ...

reveal_type(B.__class__)  # revealed: Literal[A]
```

## Invalid metaclass

If a class is a subclass of a class with a custom metaclass, then the subclass will also have that
metaclass.

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

# error: [conflicting-metaclass] "The metaclass of a derived class (`C`) must be a subclass of the metaclasses of all its bases, but `M1` and `M2` have no subclass relationship"
class C(A, B): ...

reveal_type(C.__class__)  # revealed: Unknown
```

## Conflict (2)

The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its
bases. ("Strict subclass" is a synonym for "proper subclass"; a non-strict subclass can be a
subclass or the class itself.)

```py
class M1(type): ...
class M2(type): ...
class A(metaclass=M1): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`B`) must be a subclass of the metaclasses of all its bases, but `M2` and `M1` have no subclass relationship"
class B(A, metaclass=M2): ...

reveal_type(B.__class__)  # revealed: Unknown
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

# error: [conflicting-metaclass] "The metaclass of a derived class (`D`) must be a subclass of the metaclasses of all its bases, but `M1` and `M2` have no subclass relationship"
class D(A, B, C): ...

reveal_type(D.__class__)  # revealed: Unknown
```

## Unknown

```py
from nonexistent_module import UnknownClass  # error: [unresolved-import]

class C(UnknownClass): ...

reveal_type(C.__class__)  # revealed: Literal[type]

class M(type): ...
class A(metaclass=M): ...
class B(A, UnknownClass): ...

# TODO: This should resolve to `type[M] | Unknown` instead
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

When a class has an explicit `metaclass` that is not a class, the value should be returned as is.

```py
class A(metaclass=1): ...

reveal_type(A.__class__)  # revealed: Literal[1]
```

## Cyclic

Retrieving the metaclass of a cyclically defined class should not cause an infinite loop.

```py path=a.pyi
class A(B): ...  # error: [cyclic-class-def]
class B(C): ...  # error: [cyclic-class-def]
class C(A): ...  # error: [cyclic-class-def]

reveal_type(A.__class__)  # revealed: Unknown
```

## PEP 695 generic

```py
class M(type): ...
class A[T: str](metaclass=M): ...

reveal_type(A.__class__)  # revealed: Literal[M]
```
