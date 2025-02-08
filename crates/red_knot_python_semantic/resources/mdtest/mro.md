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

reveal_type(C.__mro__)  # revealed: tuple[type[C], type[object]]
```

## The special case: `object` itself

```py
reveal_type(object.__mro__)  # revealed: tuple[type[object]]
```

## Explicit inheritance from `object`

```py
class C(object): ...

reveal_type(C.__mro__)  # revealed: tuple[type[C], type[object]]
```

## Explicit inheritance from non-`object` single base

```py
class A: ...
class B(A): ...

reveal_type(B.__mro__)  # revealed: tuple[type[B], type[A], type[object]]
```

## Linearization of multiple bases

```py
class A: ...
class B: ...
class C(A, B): ...

reveal_type(C.__mro__)  # revealed: tuple[type[C], type[A], type[B], type[object]]
```

## Complex diamond inheritance (1)

This is "ex_2" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
class O: ...
class X(O): ...
class Y(O): ...
class A(X, Y): ...
class B(Y, X): ...

reveal_type(A.__mro__)  # revealed: tuple[type[A], type[X], type[Y], type[O], type[object]]
reveal_type(B.__mro__)  # revealed: tuple[type[B], type[Y], type[X], type[O], type[object]]
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

# revealed: tuple[type[C], type[D], type[F], type[O], type[object]]
reveal_type(C.__mro__)
# revealed: tuple[type[B], type[D], type[E], type[O], type[object]]
reveal_type(B.__mro__)
# revealed: tuple[type[A], type[B], type[C], type[D], type[E], type[F], type[O], type[object]]
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

# revealed: tuple[type[C], type[D], type[F], type[O], type[object]]
reveal_type(C.__mro__)
# revealed: tuple[type[B], type[E], type[D], type[O], type[object]]
reveal_type(B.__mro__)
# revealed: tuple[type[A], type[B], type[E], type[C], type[D], type[F], type[O], type[object]]
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

# revealed: tuple[type[K1], type[A], type[B], type[C], type[O], type[object]]
reveal_type(K1.__mro__)
# revealed: tuple[type[K2], type[D], type[B], type[E], type[O], type[object]]
reveal_type(K2.__mro__)
# revealed: tuple[type[K3], type[D], type[A], type[O], type[object]]
reveal_type(K3.__mro__)
# revealed: tuple[type[Z], type[K1], type[K2], type[K3], type[D], type[A], type[B], type[C], type[E], type[O], type[object]]
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

reveal_type(A.__mro__)  # revealed: tuple[type[A], Unknown, type[object]]
reveal_type(D.__mro__)  # revealed: tuple[type[D], type[A], Unknown, type[B], type[C], type[object]]
reveal_type(E.__mro__)  # revealed: tuple[type[E], type[B], type[C], type[object]]
reveal_type(F.__mro__)  # revealed: tuple[type[F], type[E], type[B], type[C], type[A], Unknown, type[object]]
```

## `__bases__` lists that cause errors at runtime

If the class's `__bases__` cause an exception to be raised at runtime and therefore the class
creation to fail, we infer the class's `__mro__` as being `[<class>, Unknown, object]`:

```py
# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Foo` with bases list `[<class 'object'>, <class 'int'>]`"
class Foo(object, int): ...

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]

class Bar(Foo): ...

reveal_type(Bar.__mro__)  # revealed: tuple[type[Bar], type[Foo], Unknown, type[object]]

# This is the `TypeError` at the bottom of "ex_2"
# in the examples at <https://docs.python.org/3/howto/mro.html#the-end>
class O: ...
class X(O): ...
class Y(O): ...
class A(X, Y): ...
class B(Y, X): ...

reveal_type(A.__mro__)  # revealed: tuple[type[A], type[X], type[Y], type[O], type[object]]
reveal_type(B.__mro__)  # revealed: tuple[type[B], type[Y], type[X], type[O], type[object]]

# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Z` with bases list `[<class 'A'>, <class 'B'>]`"
class Z(A, B): ...

reveal_type(Z.__mro__)  # revealed: tuple[type[Z], Unknown, type[object]]

class AA(Z): ...

reveal_type(AA.__mro__)  # revealed: tuple[type[AA], type[Z], Unknown, type[object]]
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

reveal_type(x)  # revealed: types.UnionType[A, B]

# error: 11 [invalid-base] "Invalid class base with type `types.UnionType[A, B]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class Foo(x): ...

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]
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

reveal_type(x)  # revealed: types.UnionType[A, B]
reveal_type(y)  # revealed: types.UnionType[C, D]

# error: 11 [invalid-base] "Invalid class base with type `types.UnionType[A, B]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
# error: 14 [invalid-base] "Invalid class base with type `types.UnionType[C, D]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class Foo(x, y): ...

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]
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

# error: 21 [invalid-base] "Invalid class base with type `types.UnionType[Y, object]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class PossibleError(foo, X): ...

reveal_type(PossibleError.__mro__)  # revealed: tuple[type[PossibleError], Unknown, type[object]]

class A(X, Y): ...

reveal_type(A.__mro__)  # revealed: tuple[type[A], type[X], type[Y], type[O], type[object]]

if returns_bool():
    class B(X, Y): ...

else:
    class B(Y, X): ...

# revealed: tuple[type[B], type[X], type[Y], type[O], type[object]] | tuple[type[B], type[Y], type[X], type[O], type[object]]
reveal_type(B.__mro__)

# error: 12 [invalid-base] "Invalid class base with type `types.UnionType[B, B]` (all bases must be a class, `Any`, `Unknown` or `Todo`)"
class Z(A, B): ...

reveal_type(Z.__mro__)  # revealed: tuple[type[Z], Unknown, type[object]]
```

## `__bases__` lists with duplicate bases

```py
class Foo(str, str): ...  # error: 16 [duplicate-base] "Duplicate base class `str`"

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]

class Spam: ...
class Eggs: ...
class Ham(
    Spam,
    Eggs,
    Spam,  # error: [duplicate-base] "Duplicate base class `Spam`"
    Eggs,  # error: [duplicate-base] "Duplicate base class `Eggs`"
): ...

reveal_type(Ham.__mro__)  # revealed: tuple[type[Ham], Unknown, type[object]]

class Mushrooms: ...
class Omelette(Spam, Eggs, Mushrooms, Mushrooms): ...  # error: [duplicate-base]

reveal_type(Omelette.__mro__)  # revealed: tuple[type[Omelette], Unknown, type[object]]
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

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]
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

reveal_type(Foo)  # revealed: type[Foo]
reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]

class Bar: ...
class Baz: ...
class Boz(Bar, Baz, Boz): ...  # error: [cyclic-class-definition]

reveal_type(Boz)  # revealed: type[Boz]
reveal_type(Boz.__mro__)  # revealed: tuple[type[Boz], Unknown, type[object]]
```

## Classes with indirect cycles in their MROs

These are similarly unlikely, but we still shouldn't crash:

```pyi
class Foo(Bar): ...  # error: [cyclic-class-definition]
class Bar(Baz): ...  # error: [cyclic-class-definition]
class Baz(Foo): ...  # error: [cyclic-class-definition]

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]
reveal_type(Bar.__mro__)  # revealed: tuple[type[Bar], Unknown, type[object]]
reveal_type(Baz.__mro__)  # revealed: tuple[type[Baz], Unknown, type[object]]
```

## Classes with cycles in their MROs, and multiple inheritance

```pyi
class Spam: ...
class Foo(Bar): ...  # error: [cyclic-class-definition]
class Bar(Baz): ...  # error: [cyclic-class-definition]
class Baz(Foo, Spam): ...  # error: [cyclic-class-definition]

reveal_type(Foo.__mro__)  # revealed: tuple[type[Foo], Unknown, type[object]]
reveal_type(Bar.__mro__)  # revealed: tuple[type[Bar], Unknown, type[object]]
reveal_type(Baz.__mro__)  # revealed: tuple[type[Baz], Unknown, type[object]]
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

reveal_type(FooCycle.__mro__)  # revealed: tuple[type[FooCycle], Unknown, type[object]]
reveal_type(BarCycle.__mro__)  # revealed: tuple[type[BarCycle], Unknown, type[object]]
reveal_type(Baz.__mro__)  # revealed: tuple[type[Baz], Unknown, type[object]]
reveal_type(Spam.__mro__)  # revealed: tuple[type[Spam], Unknown, type[object]]
```
