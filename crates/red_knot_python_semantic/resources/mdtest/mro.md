# Mro tests

## No bases

```py
class C:
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[object]]
```

## The special case: `object` itself

```py
reveal_type(object.__mro__)  # revealed: tuple[Literal[object]]
```

## Explicit inheritance from `object`

```py
class C(object):
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[object]]
```

## Explicit inheritance from non-`object` single base

```py
class A:
    pass

class B(A):
    pass

reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[A], Literal[object]]
```

## Linearization of multiple bases

```py
class A:
    pass

class B:
    pass

class C(A, B):
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[A], Literal[B], Literal[object]]
```

## Complex diamond inheritance (1)

This is "ex_2" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O:
    pass

class X(O):
    pass

class Y(O):
    pass

class A(X, Y):
    pass

class B(Y, X):
    pass

reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[X], Literal[Y], Literal[O], Literal[object]]
reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[Y], Literal[X], Literal[O], Literal[object]]
```

## Complex diamond inheritance (2)

This is "ex_5" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O:
    pass

class F(O):
    pass

class E(O):
    pass

class D(O):
    pass

class C(D, F):
    pass

class B(D, E):
    pass

class A(B, C):
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[D], Literal[E], Literal[O], Literal[object]]
reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[B], Literal[C], Literal[D], Literal[E], Literal[F], Literal[O], Literal[object]]
```

## Complex diamond inheritance (3)

This is "ex_6" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O:
    pass

class F(O):
    pass

class E(O):
    pass

class D(O):
    pass

class C(D, F):
    pass

class B(E, D):
    pass

class A(B, C):
    pass

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[E], Literal[D], Literal[O], Literal[object]]
reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[B], Literal[E], Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
```

## Complex diamond inheritance (4)

This is "ex_9" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O:
    pass

class A(O):
    pass

class B(O):
    pass

class C(O):
    pass

class D(O):
    pass

class E(O):
    pass

class K1(A, B, C):
    pass

class K2(D, B, E):
    pass

class K3(D, A):
    pass

class Z(K1, K2, K3):
    pass

reveal_type(K1.__mro__)  # revealed: tuple[Literal[K1], Literal[A], Literal[B], Literal[C], Literal[O], Literal[object]]
reveal_type(K2.__mro__)  # revealed: tuple[Literal[K2], Literal[D], Literal[B], Literal[E], Literal[O], Literal[object]]
reveal_type(K3.__mro__)  # revealed: tuple[Literal[K3], Literal[D], Literal[A], Literal[O], Literal[object]]
reveal_type(Z.__mro__)  # revealed: tuple[Literal[Z], Literal[K1], Literal[K2], Literal[K3], Literal[D], Literal[A], Literal[B], Literal[C], Literal[E], Literal[O], Literal[object]]
```

## Inheritance from `Unknown`

```py
from does_not_exist import DoesNotExist  # error: [unresolved-import]

class A(DoesNotExist):
    pass

class B:
    pass

class C:
    pass

class D(A, B, C):
    pass

class E(B, C):
    pass

class F(E, A):
    pass

reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Unknown, Literal[object]]
reveal_type(D.__mro__)  # revealed: tuple[Literal[D], Literal[A], Unknown, Literal[B], Literal[C], Literal[object]]
reveal_type(E.__mro__)  # revealed: tuple[Literal[E], Literal[B], Literal[C], Literal[object]]
reveal_type(F.__mro__)  # revealed: tuple[Literal[F], Literal[E], Literal[B], Literal[C], Literal[A], Unknown, Literal[object]]
```

## `__bases__` includes a `Union`

```py
def returns_bool() -> bool:
    return True

class A:
    pass

class B:
    pass

if returns_bool():
    x = A
else:
    x = B

reveal_type(x)  # revealed: Literal[A, B]

class Foo(x):
    pass

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Literal[A], Literal[object]] | tuple[Literal[Foo], Literal[B], Literal[object]]
```

## `__bases__` includes multiple `Union`s

```py
def returns_bool() -> bool:
    return True

class A:
    pass

class B:
    pass

class C:
    pass

class D:
    pass

if returns_bool():
    x = A
else:
    x = B

if returns_bool():
    y = C
else:
    y = D

reveal_type(x)  # revealed: Literal[A, B]
reveal_type(y)  # revealed: Literal[C, D]

class Foo(x, y):
    pass

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Literal[A], Literal[D], Literal[object]] | tuple[Literal[Foo], Literal[B], Literal[D], Literal[object]] | tuple[Literal[Foo], Literal[A], Literal[C], Literal[object]] | tuple[Literal[Foo], Literal[B], Literal[C], Literal[object]]
```
