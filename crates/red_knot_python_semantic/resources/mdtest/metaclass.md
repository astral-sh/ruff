## Default

```py
class C(type): ...

reveal_type(C.__class__)  # revealed: Literal[type]
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
class A: ...
class B(metaclass=A): ...

# TODO: emit a diagnostic for the invalid metaclass
reveal_type(B.__class__)  # revealed: Literal[A]
```

## Linear inheritance

If a class is a subclass of a class with a custom metaclass, then the subclass will also have that
metaclass.

```py
class A(type): ...
class B(metaclass=A): ...
class C(B): ...

reveal_type(C.__class__)  # revealed: Literal[A]
```

## Conflict (1)

The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its
bases. ("Strict subclass" is a synonym for "proper subclass"; a non-strict subclass can be a
subclass or the class itself.)

```py
class A(type): ...
class B(metaclass=A): ...
class C(type): ...
class D(metaclass=C): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`E`) must be a subclass of the metaclasses of all its bases, but `Literal[C]` and `Literal[A]` have no subclass relationship"
class E(D, B): ...

reveal_type(E.__class__)  # revealed: Unknown
```

## Conflict (2)

The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its
bases. ("Strict subclass" is a synonym for "proper subclass"; a non-strict subclass can be a
subclass or the class itself.)

```py
class A(type): ...
class B(metaclass=A): ...
class C(type): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`E`) must be a subclass of the metaclasses of all its bases, but `Literal[C]` and `Literal[A]` have no subclass relationship"
class E(B, metaclass=C): ...

reveal_type(E.__class__)  # revealed: Unknown
```

## Inheritance (1)

```py
class A(type): ...
class B(metaclass=A): ...
class C(metaclass=A): ...
class D(B, C): ...

reveal_type(D.__class__)  # revealed: Literal[A]
```

## Inheritance (2)

```py
class A(type): ...
class B(metaclass=A): ...
class C(metaclass=A): ...
class D(C, B): ...

reveal_type(D.__class__)  # revealed: Literal[A]
```

## Inheritance (3)

```py
class A(type): ...
class B(metaclass=A): ...
class C(B): ...
class D(metaclass=C): ...
class E(D): ...

reveal_type(E.__class__)  # revealed: Literal[C]
```

## Inheritance (4)

```py
class M(type): ...
class N(M): ...
class A(metaclass=M): ...
class B(metaclass=N): ...
class C(A, B): ...

reveal_type(C.__class__)  # revealed: Literal[N]
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

# error: [conflicting-metaclass] "The metaclass of a derived class (`D`) must be a subclass of the metaclasses of all its bases, but `Literal[M1]` and `Literal[M2]` have no subclass relationship"
class D(A, B, C): ...

reveal_type(D.__class__)  # revealed: Unknown
```

## Duplicate

```py
class A(type): ...
class B(metaclass=A): ...
class C(B, B): ...  # error: [duplicate-base] "Duplicate base class `B`"

reveal_type(C.__class__)  # revealed: Literal[A]
```

## Non-class

```py
class A(metaclass=None): ...

reveal_type(A.__class__)  # revealed: None
```
