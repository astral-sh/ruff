## Default

```py
class C: ...

reveal_type(C.__class__)  # revealed: Literal[type]
```

## Basic

```py
class A: ...
class B(metaclass=A): ...

reveal_type(B.__class__)  # revealed: Literal[A]
```

## Linear inheritance (1)

If a class is a subclass of a class with a custom metaclass, then the subclass will also have that
metaclass.

```py
class A: ...
class B(metaclass=A): ...
class C(B): ...

reveal_type(C.__class__)  # revealed: Literal[A]
```

## Linear inheritance (2)

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
bases.

```py
class A: ...
class B(metaclass=A): ...
class C: ...
class D(metaclass=C): ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`E`) must be a subclass of the metaclasses of all its bases, but `Literal[C]` and `Literal[A]` are not compatible"
class E(D, B): ...

reveal_type(E.__class__)  # revealed: Unknown
```

## Conflict (2)

The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its
bases.

```py
class A: ...
class B(metaclass=A): ...
class C: ...

# error: [conflicting-metaclass] "The metaclass of a derived class (`E`) must be a subclass of the metaclasses of all its bases, but `Literal[C]` and `Literal[A]` are not compatible"
class E(B, metaclass=C): ...

reveal_type(E.__class__)  # revealed: Unknown
```

## Inheritance (1)

```py
class A: ...
class B(metaclass=A): ...
class C(metaclass=A): ...
class D(B, C): ...

reveal_type(D.__class__)  # revealed: Literal[A]
```

## Inheritance (2)

```py
class A: ...
class B(metaclass=A): ...
class C(metaclass=A): ...
class D(C, B): ...

reveal_type(D.__class__)  # revealed: Literal[A]
```

## Inheritance (3)

```py
class A: ...
class B(metaclass=A): ...
class C(B): ...
class D(metaclass=C): ...
class E(D): ...

reveal_type(E.__class__)  # revealed: Literal[C]
```

## Inheritance (4)

```py
class A: ...
class B(metaclass=A): ...
class C(metaclass=B): ...
class D(C): ...

reveal_type(D.__class__)  # revealed: Literal[B]
```
