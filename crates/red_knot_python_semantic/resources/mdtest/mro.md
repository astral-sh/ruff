# Method Resolution Order tests

Tests that assert that we can infer the correct type for a class's `__mro__` attribute.

This attribute is rarely accessed directly at runtime. However, it's extremely important for *us* to
know the precise possible values of a class's Method Resolution Order, or we won't be able to infer
the correct type of attributes accessed from instances.

For documentation on method resolution orders, see:

- <https://docs.python.org/3/glossary.html#term-method-resolution-order>
- <https://docs.python.org/3/howto/mro.html#python-2-3-mro>

## No bases

```py
class C: ...

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[object]]
```

## The special case: `object` itself

```py
reveal_type(object.__mro__)  # revealed: tuple[Literal[object]]
```

## Explicit inheritance from `object`

```py
class C(object): ...

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[object]]
```

## Explicit inheritance from non-`object` single base

```py
class A: ...
class B(A): ...

reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[A], Literal[object]]
```

## Linearization of multiple bases

```py
class A: ...
class B: ...
class C(A, B): ...

reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[A], Literal[B], Literal[object]]
```

## Complex diamond inheritance (1)

This is "ex_2" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O: ...
class X(O): ...
class Y(O): ...
class A(X, Y): ...
class B(Y, X): ...

reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[X], Literal[Y], Literal[O], Literal[object]]
reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[Y], Literal[X], Literal[O], Literal[object]]
```

## Complex diamond inheritance (2)

This is "ex_5" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O: ...
class F(O): ...
class E(O): ...
class D(O): ...
class C(D, F): ...
class B(D, E): ...
class A(B, C): ...

# revealed: tuple[Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
reveal_type(C.__mro__)
# revealed: tuple[Literal[B], Literal[D], Literal[E], Literal[O], Literal[object]]
reveal_type(B.__mro__)
# revealed: tuple[Literal[A], Literal[B], Literal[C], Literal[D], Literal[E], Literal[F], Literal[O], Literal[object]]
reveal_type(A.__mro__)
```

## Complex diamond inheritance (3)

This is "ex_6" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O: ...
class F(O): ...
class E(O): ...
class D(O): ...
class C(D, F): ...
class B(E, D): ...
class A(B, C): ...

# revealed: tuple[Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
reveal_type(C.__mro__)
# revealed: tuple[Literal[B], Literal[E], Literal[D], Literal[O], Literal[object]]
reveal_type(B.__mro__)
# revealed: tuple[Literal[A], Literal[B], Literal[E], Literal[C], Literal[D], Literal[F], Literal[O], Literal[object]]
reveal_type(A.__mro__)
```

## Complex diamond inheritance (4)

This is "ex_9" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O: ...
class A(O): ...
class B(O): ...
class C(O): ...
class D(O): ...
class E(O): ...
class K1(A, B, C): ...
class K2(D, B, E): ...
class K3(D, A): ...
class Z(K1, K2, K3): ...

# revealed: tuple[Literal[K1], Literal[A], Literal[B], Literal[C], Literal[O], Literal[object]]
reveal_type(K1.__mro__)
# revealed: tuple[Literal[K2], Literal[D], Literal[B], Literal[E], Literal[O], Literal[object]]
reveal_type(K2.__mro__)
# revealed: tuple[Literal[K3], Literal[D], Literal[A], Literal[O], Literal[object]]
reveal_type(K3.__mro__)
# revealed: tuple[Literal[Z], Literal[K1], Literal[K2], Literal[K3], Literal[D], Literal[A], Literal[B], Literal[C], Literal[E], Literal[O], Literal[object]]
reveal_type(Z.__mro__)
```

## Inheritance from `Unknown`

```py
from does_not_exist import DoesNotExist  # error: [unresolved-import]

class A(DoesNotExist): ...
class B: ...
class C: ...
class D(A, B, C): ...
class E(B, C): ...
class F(E, A): ...

reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Unknown, Literal[object]]
reveal_type(D.__mro__)  # revealed: tuple[Literal[D], Literal[A], Unknown, Literal[B], Literal[C], Literal[object]]
reveal_type(E.__mro__)  # revealed: tuple[Literal[E], Literal[B], Literal[C], Literal[object]]
reveal_type(F.__mro__)  # revealed: tuple[Literal[F], Literal[E], Literal[B], Literal[C], Literal[A], Unknown, Literal[object]]
```

## `__bases__` lists that cause errors at runtime

If the class's `__bases__` cause an exception to be raised at runtime and therefore the class
creation to fail, we infer the class's `__mro__` as being `[<class>, Unknown, object]`:

```py
# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Foo` with bases list `[<class 'object'>, <class 'int'>]`"
class Foo(object, int): ...

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]

class Bar(Foo): ...

reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Literal[Foo], Unknown, Literal[object]]

# This is the `TypeError` at the bottom of "ex_2"
# in the examples at <https://docs.python.org/3/howto/mro.html#the-end>
class O: ...
class X(O): ...
class Y(O): ...
class A(X, Y): ...
class B(Y, X): ...

reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[X], Literal[Y], Literal[O], Literal[object]]
reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[Y], Literal[X], Literal[O], Literal[object]]

# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Z` with bases list `[<class 'A'>, <class 'B'>]`"
class Z(A, B): ...

reveal_type(Z.__mro__)  # revealed: tuple[Literal[Z], Unknown, Literal[object]]

class AA(Z): ...

reveal_type(AA.__mro__)  # revealed: tuple[Literal[AA], Literal[Z], Unknown, Literal[object]]
```

## `__bases__` includes a `Union`

We don't support union types in a class's bases; a base must resolve to a single `ClassLiteralType`.
If we find a union type in a class's bases, we infer the class's `__mro__` as being
`[<class>, Unknown, object]`, the same as for MROs that cause errors at runtime.

```py
def returns_bool() -> bool:
    return True

class A: ...
class B: ...

if returns_bool():
    x = A
else:
    x = B

reveal_type(x)  # revealed: Literal[A, B]

# error: 11 [invalid-base] "Invalid class base with type `Literal[A, B]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class Foo(x): ...

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]
```

## `__bases__` includes multiple `Union`s

```py
def returns_bool() -> bool:
    return True

class A: ...
class B: ...
class C: ...
class D: ...

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

# error: 11 [invalid-base] "Invalid class base with type `Literal[A, B]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
# error: 14 [invalid-base] "Invalid class base with type `Literal[C, D]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class Foo(x, y): ...

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]
```

## `__bases__` lists that cause errors... now with `Union`s

```py
def returns_bool() -> bool:
    return True

class O: ...
class X(O): ...
class Y(O): ...

if returns_bool():
    foo = Y
else:
    foo = object

# error: 21 [invalid-base] "Invalid class base with type `Literal[Y, object]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class PossibleError(foo, X): ...

reveal_type(PossibleError.__mro__)  # revealed: tuple[Literal[PossibleError], Unknown, Literal[object]]

class A(X, Y): ...

reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[X], Literal[Y], Literal[O], Literal[object]]

if returns_bool():
    class B(X, Y): ...

else:
    class B(Y, X): ...

# revealed: tuple[Literal[B], Literal[X], Literal[Y], Literal[O], Literal[object]] | tuple[Literal[B], Literal[Y], Literal[X], Literal[O], Literal[object]]
reveal_type(B.__mro__)

# error: 12 [invalid-base] "Invalid class base with type `Literal[B, B]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class Z(A, B): ...

reveal_type(Z.__mro__)  # revealed: tuple[Literal[Z], Unknown, Literal[object]]
```

## `__bases__` lists with duplicate bases

```py
class Foo(str, str): ...  # error: 16 [duplicate-base] "Duplicate base class `str`"

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]

class Spam: ...
class Eggs: ...
class Ham(
    Spam,
    Eggs,
    Spam,  # error: [duplicate-base] "Duplicate base class `Spam`"
    Eggs,  # error: [duplicate-base] "Duplicate base class `Eggs`"
): ...

reveal_type(Ham.__mro__)  # revealed: tuple[Literal[Ham], Unknown, Literal[object]]

class Mushrooms: ...
class Omelette(Spam, Eggs, Mushrooms, Mushrooms): ...  # error: [duplicate-base]

reveal_type(Omelette.__mro__)  # revealed: tuple[Literal[Omelette], Unknown, Literal[object]]
```

## `__bases__` lists with duplicate `Unknown` bases

```py
# error: [unresolved-import]
# error: [unresolved-import]
from does_not_exist import unknown_object_1, unknown_object_2

reveal_type(unknown_object_1)  # revealed: Unknown
reveal_type(unknown_object_2)  # revealed: Unknown

# We *should* emit an error here to warn the user that we have no idea
# what the MRO of this class should really be.
# However, we don't complain about "duplicate base classes" here,
# even though two classes are both inferred as being `Unknown`.
#
# (TODO: should we revisit this? Does it violate the gradual guarantee?
# Should we just silently infer `[Foo, Unknown, object]` as the MRO here
# without emitting any error at all? Not sure...)
#
# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Foo` with bases list `[Unknown, Unknown]`"
class Foo(unknown_object_1, unknown_object_2): ...

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]
```

## Unrelated objects inferred as `Any`/`Unknown` do not have special `__mro__` attributes

```py
from does_not_exist import unknown_object  # error: [unresolved-import]

reveal_type(unknown_object)  # revealed: Unknown
reveal_type(unknown_object.__mro__)  # revealed: Unknown
```

## Classes that inherit from themselves

These are invalid, but we need to be able to handle them gracefully without panicking.

```pyi
class Foo(Foo): ...  # error: [cyclic-class-definition]

reveal_type(Foo)  # revealed: Literal[Foo]
reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]

class Bar: ...
class Baz: ...
class Boz(Bar, Baz, Boz): ...  # error: [cyclic-class-definition]

reveal_type(Boz)  # revealed: Literal[Boz]
reveal_type(Boz.__mro__)  # revealed: tuple[Literal[Boz], Unknown, Literal[object]]
```

## Classes with indirect cycles in their MROs

These are similarly unlikely, but we still shouldn't crash:

```pyi
class Foo(Bar): ...  # error: [cyclic-class-definition]
class Bar(Baz): ...  # error: [cyclic-class-definition]
class Baz(Foo): ...  # error: [cyclic-class-definition]

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]
reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Unknown, Literal[object]]
reveal_type(Baz.__mro__)  # revealed: tuple[Literal[Baz], Unknown, Literal[object]]
```

## Classes with cycles in their MROs, and multiple inheritance

```pyi
class Spam: ...
class Foo(Bar): ...  # error: [cyclic-class-definition]
class Bar(Baz): ...  # error: [cyclic-class-definition]
class Baz(Foo, Spam): ...  # error: [cyclic-class-definition]

reveal_type(Foo.__mro__)  # revealed: tuple[Literal[Foo], Unknown, Literal[object]]
reveal_type(Bar.__mro__)  # revealed: tuple[Literal[Bar], Unknown, Literal[object]]
reveal_type(Baz.__mro__)  # revealed: tuple[Literal[Baz], Unknown, Literal[object]]
```

## Classes with cycles in their MRO, and a sub-graph

```pyi
class FooCycle(BarCycle): ...  # error: [cyclic-class-definition]
class Foo: ...
class BarCycle(FooCycle): ...  # error: [cyclic-class-definition]
class Bar(Foo): ...

# Avoid emitting the errors for these. The classes have cyclic superclasses,
# but are not themselves cyclic...
class Baz(Bar, BarCycle): ...
class Spam(Baz): ...

reveal_type(FooCycle.__mro__)  # revealed: tuple[Literal[FooCycle], Unknown, Literal[object]]
reveal_type(BarCycle.__mro__)  # revealed: tuple[Literal[BarCycle], Unknown, Literal[object]]
reveal_type(Baz.__mro__)  # revealed: tuple[Literal[Baz], Unknown, Literal[object]]
reveal_type(Spam.__mro__)  # revealed: tuple[Literal[Spam], Unknown, Literal[object]]
```
