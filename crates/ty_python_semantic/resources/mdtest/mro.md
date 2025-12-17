# Method Resolution Order tests

Tests that assert that we can infer the correct MRO for a class.

It's extremely important for us to know the precise possible values of a class's Method Resolution
Order, or we won't be able to infer the correct type of attributes accessed from instances.

For documentation on method resolution orders, see:

- <https://docs.python.org/3/glossary.html#term-method-resolution-order>
- <https://docs.python.org/3/howto/mro.html#python-2-3-mro>

At runtime, the MRO for a class can be inspected using the `__mro__` attribute. However, rather than
special-casing inference of that attribute, we allow our inferred MRO of a class to be introspected
using the `ty_extensions.reveal_mro` function. This is because the MRO ty infers for a class will
often be different than a class's "real MRO" at runtime. This is often deliberate and desirable, but
would be confusing to users. For example, typeshed pretends that builtin sequences such as `tuple`
and `list` inherit from `collections.abc.Sequence`, resulting in a much longer inferred MRO for
these classes than what they actually have at runtime. Other differences to "real MROs" at runtime
include the facts that ty's inferred MRO will often include non-class elements, such as generic
aliases, `Any` and `Unknown`.

## No bases

```py
from ty_extensions import reveal_mro

class C: ...

reveal_mro(C)  # revealed: (<class 'C'>, <class 'object'>)
```

## The special case: `object` itself

```py
from ty_extensions import reveal_mro

reveal_mro(object)  # revealed: (<class 'object'>,)
```

## Explicit inheritance from `object`

```py
from ty_extensions import reveal_mro

class C(object): ...

reveal_mro(C)  # revealed: (<class 'C'>, <class 'object'>)
```

## Explicit inheritance from non-`object` single base

```py
from ty_extensions import reveal_mro

class A: ...
class B(A): ...

reveal_mro(B)  # revealed: (<class 'B'>, <class 'A'>, <class 'object'>)
```

## Linearization of multiple bases

```py
from ty_extensions import reveal_mro

class A: ...
class B: ...
class C(A, B): ...

reveal_mro(C)  # revealed: (<class 'C'>, <class 'A'>, <class 'B'>, <class 'object'>)
```

## Complex diamond inheritance (1)

This is "ex_2" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
from ty_extensions import reveal_mro

class O: ...
class X(O): ...
class Y(O): ...
class A(X, Y): ...
class B(Y, X): ...

reveal_mro(A)  # revealed: (<class 'A'>, <class 'X'>, <class 'Y'>, <class 'O'>, <class 'object'>)
reveal_mro(B)  # revealed: (<class 'B'>, <class 'Y'>, <class 'X'>, <class 'O'>, <class 'object'>)
```

## Complex diamond inheritance (2)

This is "ex_5" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
from ty_extensions import reveal_mro

class O: ...
class F(O): ...
class E(O): ...
class D(O): ...
class C(D, F): ...
class B(D, E): ...
class A(B, C): ...

# revealed: (<class 'C'>, <class 'D'>, <class 'F'>, <class 'O'>, <class 'object'>)
reveal_mro(C)
# revealed: (<class 'B'>, <class 'D'>, <class 'E'>, <class 'O'>, <class 'object'>)
reveal_mro(B)
# revealed: (<class 'A'>, <class 'B'>, <class 'C'>, <class 'D'>, <class 'E'>, <class 'F'>, <class 'O'>, <class 'object'>)
reveal_mro(A)
```

## Complex diamond inheritance (3)

This is "ex_6" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
from ty_extensions import reveal_mro

class O: ...
class F(O): ...
class E(O): ...
class D(O): ...
class C(D, F): ...
class B(E, D): ...
class A(B, C): ...

# revealed: (<class 'C'>, <class 'D'>, <class 'F'>, <class 'O'>, <class 'object'>)
reveal_mro(C)
# revealed: (<class 'B'>, <class 'E'>, <class 'D'>, <class 'O'>, <class 'object'>)
reveal_mro(B)
# revealed: (<class 'A'>, <class 'B'>, <class 'E'>, <class 'C'>, <class 'D'>, <class 'F'>, <class 'O'>, <class 'object'>)
reveal_mro(A)
```

## Complex diamond inheritance (4)

This is "ex_9" from <https://docs.python.org/3/howto/mro.html#the-end>

```py
from ty_extensions import reveal_mro

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

# revealed: (<class 'K1'>, <class 'A'>, <class 'B'>, <class 'C'>, <class 'O'>, <class 'object'>)
reveal_mro(K1)
# revealed: (<class 'K2'>, <class 'D'>, <class 'B'>, <class 'E'>, <class 'O'>, <class 'object'>)
reveal_mro(K2)
# revealed: (<class 'K3'>, <class 'D'>, <class 'A'>, <class 'O'>, <class 'object'>)
reveal_mro(K3)
# revealed: (<class 'Z'>, <class 'K1'>, <class 'K2'>, <class 'K3'>, <class 'D'>, <class 'A'>, <class 'B'>, <class 'C'>, <class 'E'>, <class 'O'>, <class 'object'>)
reveal_mro(Z)
```

## Inheritance from `Unknown`

```py
from ty_extensions import reveal_mro
from does_not_exist import DoesNotExist  # error: [unresolved-import]

class A(DoesNotExist): ...
class B: ...
class C: ...
class D(A, B, C): ...
class E(B, C): ...
class F(E, A): ...

reveal_mro(A)  # revealed: (<class 'A'>, Unknown, <class 'object'>)
reveal_mro(D)  # revealed: (<class 'D'>, <class 'A'>, Unknown, <class 'B'>, <class 'C'>, <class 'object'>)
reveal_mro(E)  # revealed: (<class 'E'>, <class 'B'>, <class 'C'>, <class 'object'>)
# revealed: (<class 'F'>, <class 'E'>, <class 'B'>, <class 'C'>, <class 'A'>, Unknown, <class 'object'>)
reveal_mro(F)
```

## Inheritance with intersections that include `Unknown`

An intersection that includes `Unknown` or `Any` is permitted as long as the intersection is not
disjoint from `type`.

```py
from ty_extensions import reveal_mro
from does_not_exist import DoesNotExist  # error: [unresolved-import]

reveal_type(DoesNotExist)  # revealed: Unknown

if hasattr(DoesNotExist, "__mro__"):
    reveal_type(DoesNotExist)  # revealed: Unknown & <Protocol with members '__mro__'>

    class Foo(DoesNotExist): ...  # no error!
    reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)

if not isinstance(DoesNotExist, type):
    reveal_type(DoesNotExist)  # revealed: Unknown & ~type

    class Foo(DoesNotExist): ...  # error: [unsupported-base]
    reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)
```

## Inheritance from `type[Any]` and `type[Unknown]`

Inheritance from `type[Any]` and `type[Unknown]` is also permitted, in keeping with the gradual
guarantee:

```py
from typing import Any
from ty_extensions import Unknown, Intersection, reveal_mro

def f(x: type[Any], y: Intersection[Unknown, type[Any]]):
    class Foo(x): ...
    reveal_mro(Foo)  # revealed: (<class 'Foo'>, Any, <class 'object'>)

    class Bar(y): ...
    reveal_mro(Bar)  # revealed: (<class 'Bar'>, Unknown, <class 'object'>)
```

## `__bases__` lists that cause errors at runtime

If the class's `__bases__` cause an exception to be raised at runtime and therefore the class
creation to fail, we infer the class's `__mro__` as being `[<class>, Unknown, object]`:

```py
from ty_extensions import reveal_mro

# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Foo` with bases list `[<class 'object'>, <class 'int'>]`"
class Foo(object, int): ...

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)

class Bar(Foo): ...

reveal_mro(Bar)  # revealed: (<class 'Bar'>, <class 'Foo'>, Unknown, <class 'object'>)

# This is the `TypeError` at the bottom of "ex_2"
# in the examples at <https://docs.python.org/3/howto/mro.html#the-end>
class O: ...
class X(O): ...
class Y(O): ...
class A(X, Y): ...
class B(Y, X): ...

reveal_mro(A)  # revealed: (<class 'A'>, <class 'X'>, <class 'Y'>, <class 'O'>, <class 'object'>)
reveal_mro(B)  # revealed: (<class 'B'>, <class 'Y'>, <class 'X'>, <class 'O'>, <class 'object'>)

# error: [inconsistent-mro] "Cannot create a consistent method resolution order (MRO) for class `Z` with bases list `[<class 'A'>, <class 'B'>]`"
class Z(A, B): ...

reveal_mro(Z)  # revealed: (<class 'Z'>, Unknown, <class 'object'>)

class AA(Z): ...

reveal_mro(AA)  # revealed: (<class 'AA'>, <class 'Z'>, Unknown, <class 'object'>)
```

## `__bases__` includes a `Union`

<!-- snapshot-diagnostics -->

We don't support union types in a class's bases; a base must resolve to a single `ClassType`. If we
find a union type in a class's bases, we infer the class's `__mro__` as being
`[<class>, Unknown, object]`, the same as for MROs that cause errors at runtime.

```py
from ty_extensions import reveal_mro

def returns_bool() -> bool:
    return True

class A: ...
class B: ...

if returns_bool():
    x = A
else:
    x = B

reveal_type(x)  # revealed: <class 'A'> | <class 'B'>

# error: 11 [unsupported-base] "Unsupported class base with type `<class 'A'> | <class 'B'>`"
class Foo(x): ...

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)

def f():
    if returns_bool():
        class C: ...
    else:
        class C: ...

    class D(C): ...  # error: [unsupported-base]
```

## `UnionType` instances are now allowed as a base

This is not legal:

```py
class A: ...
class B: ...

EitherOr = A | B

# error: [invalid-base] "Invalid class base with type `<types.UnionType special-form 'A | B'>`"
class Foo(EitherOr): ...
```

## `__bases__` is a union of a dynamic type and valid bases

If a dynamic type such as `Any` or `Unknown` is one of the elements in the union, and all other
types *would be* valid class bases, we do not emit an `invalid-base` or `unsupported-base`
diagnostic, and we use the dynamic type as a base to prevent further downstream errors.

```py
from typing import Any
from ty_extensions import reveal_mro

def _(flag: bool, any: Any):
    if flag:
        Base = any
    else:
        class Base: ...

    class Foo(Base): ...
    reveal_mro(Foo)  # revealed: (<class 'Foo'>, Any, <class 'object'>)
```

## `__bases__` includes multiple `Union`s

```py
from ty_extensions import reveal_mro

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

reveal_type(x)  # revealed: <class 'A'> | <class 'B'>
reveal_type(y)  # revealed: <class 'C'> | <class 'D'>

# error: 11 [unsupported-base] "Unsupported class base with type `<class 'A'> | <class 'B'>`"
# error: 14 [unsupported-base] "Unsupported class base with type `<class 'C'> | <class 'D'>`"
class Foo(x, y): ...

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)
```

## `__bases__` lists that cause errors... now with `Union`s

```py
from ty_extensions import reveal_mro

def returns_bool() -> bool:
    return True

class O: ...
class X(O): ...
class Y(O): ...

if returns_bool():
    foo = Y
else:
    foo = object

# error: 21 [unsupported-base] "Unsupported class base with type `<class 'Y'> | <class 'object'>`"
class PossibleError(foo, X): ...

reveal_mro(PossibleError)  # revealed: (<class 'PossibleError'>, Unknown, <class 'object'>)

class A(X, Y): ...

reveal_mro(A)  # revealed: (<class 'A'>, <class 'X'>, <class 'Y'>, <class 'O'>, <class 'object'>)

if returns_bool():
    class B(X, Y): ...

else:
    class B(Y, X): ...

# revealed: (<class 'B'>, <class 'X'>, <class 'Y'>, <class 'O'>, <class 'object'>) | (<class 'B'>, <class 'Y'>, <class 'X'>, <class 'O'>, <class 'object'>)
reveal_mro(B)

# error: 12 [unsupported-base] "Unsupported class base with type `<class 'mdtest_snippet.B @ src/mdtest_snippet.py:25'> | <class 'mdtest_snippet.B @ src/mdtest_snippet.py:28'>`"
class Z(A, B): ...

reveal_mro(Z)  # revealed: (<class 'Z'>, Unknown, <class 'object'>)
```

## `__bases__` lists that include objects that are not instances of `type`

<!-- snapshot-diagnostics -->

```py
class Foo(2): ...  # error: [invalid-base]
```

A base that is not an instance of `type` but does have an `__mro_entries__` method will not raise an
exception at runtime, so we issue `unsupported-base` rather than `invalid-base`:

```py
class Foo:
    def __mro_entries__(self, bases: tuple[type, ...]) -> tuple[type, ...]:
        return ()

class Bar(Foo()): ...  # error: [unsupported-base]
```

But for objects that have badly defined `__mro_entries__`, `invalid-base` is emitted rather than
`unsupported-base`:

```py
class Bad1:
    def __mro_entries__(self, bases, extra_arg):
        return ()

class Bad2:
    def __mro_entries__(self, bases) -> int:
        return 42

class BadSub1(Bad1()): ...  # error: [invalid-base]
class BadSub2(Bad2()): ...  # error: [invalid-base]
```

## `__bases__` lists with duplicate bases

<!-- snapshot-diagnostics -->

```py
from ty_extensions import reveal_mro

class Foo(str, str): ...  # error: [duplicate-base] "Duplicate base class `str`"

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)

class Spam: ...
class Eggs: ...
class Bar: ...
class Baz: ...

# fmt: off

# error: [duplicate-base] "Duplicate base class `Spam`"
# error: [duplicate-base] "Duplicate base class `Eggs`"
class Ham(
    Spam,
    Eggs,
    Bar,
    Baz,
    Spam,
    Eggs,
): ...

# fmt: on

reveal_mro(Ham)  # revealed: (<class 'Ham'>, Unknown, <class 'object'>)

class Mushrooms: ...
class Omelette(Spam, Eggs, Mushrooms, Mushrooms): ...  # error: [duplicate-base]

reveal_mro(Omelette)  # revealed: (<class 'Omelette'>, Unknown, <class 'object'>)

# fmt: off

# error: [duplicate-base] "Duplicate base class `Eggs`"
class VeryEggyOmelette(
    Eggs,
    Ham,
    Spam,
    Eggs,
    Mushrooms,
    Bar,
    Eggs,
    Baz,
    Eggs,
): ...

# fmt: off
```

A `type: ignore` comment can suppress `duplicate-bases` errors if it is on the first or last line of
the class "header":

```py
# fmt: off

class A: ...

class B(  # type: ignore[duplicate-base]
    A,
    A,
): ...

class C(
    A,
    A
):  # type: ignore[duplicate-base]
    x: int

# fmt: on
```

But it will not suppress the error if it occurs in the class body, or on the duplicate base itself.
The justification for this is that it is the class definition as a whole that will raise an
exception at runtime, not a sub-expression in the class's bases list.

```py
# fmt: off

# error: [duplicate-base]
class D(
    A,
    # error: [unused-ignore-comment]
    A,  # type: ignore[duplicate-base]
): ...

# error: [duplicate-base]
class E(
    A,
    A
):
    # error: [unused-ignore-comment]
    x: int  # type: ignore[duplicate-base]

# fmt: on
```

## `__bases__` lists with duplicate `Unknown` bases

We do not emit errors on classes where multiple bases are inferred as `Unknown`, `Todo` or `Any`.
Usually having duplicate bases in a bases list like this would cause us to emit a diagnostic;
however, for gradual types this would break the
[gradual guarantee](https://typing.python.org/en/latest/spec/concepts.html#the-gradual-guarantee):
the dynamic base can usually be materialised to a type that would lead to a resolvable MRO.

```py
from ty_extensions import reveal_mro
from unresolvable_module import UnknownBase1, UnknownBase2  # error: [unresolved-import]

reveal_type(UnknownBase1)  # revealed: Unknown
reveal_type(UnknownBase2)  # revealed: Unknown

# no error here -- we respect the gradual guarantee:
class Foo(UnknownBase1, UnknownBase2): ...

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)
```

However, if there are duplicate class elements, we do emit an error, even if there are also multiple
dynamic members. The following class definition will definitely fail, no matter what the dynamic
bases materialize to:

```py
# error: [duplicate-base] "Duplicate base class `Foo`"
class Bar(UnknownBase1, Foo, UnknownBase2, Foo): ...

reveal_mro(Bar)  # revealed: (<class 'Bar'>, Unknown, <class 'object'>)
```

## Unrelated objects inferred as `Any`/`Unknown` do not have special `__mro__` attributes

```py
from does_not_exist import unknown_object  # error: [unresolved-import]

reveal_type(unknown_object)  # revealed: Unknown
reveal_type(unknown_object.__mro__)  # revealed: Unknown
```

## MROs of classes that use multiple inheritance with generic aliases and subscripted `Generic`

```py
from typing import Generic, TypeVar, Iterator
from ty_extensions import reveal_mro

T = TypeVar("T")

class peekable(Generic[T], Iterator[T]): ...

# revealed: (<class 'peekable[Unknown]'>, <class 'Iterator[T@peekable]'>, <class 'Iterable[T@peekable]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(peekable)

class peekable2(Iterator[T], Generic[T]): ...

# revealed: (<class 'peekable2[Unknown]'>, <class 'Iterator[T@peekable2]'>, <class 'Iterable[T@peekable2]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(peekable2)

class Base: ...
class Intermediate(Base, Generic[T]): ...
class Sub(Intermediate[T], Base): ...

# revealed: (<class 'Sub[Unknown]'>, <class 'Intermediate[T@Sub]'>, <class 'Base'>, typing.Generic, <class 'object'>)
reveal_mro(Sub)
```

## Unresolvable MROs involving generics have the original bases reported in the error message, not the resolved bases

<!-- snapshot-diagnostics -->

```py
from typing_extensions import Protocol, TypeVar, Generic

T = TypeVar("T")

class Foo(Protocol): ...
class Bar(Protocol[T]): ...
class Baz(Protocol[T], Foo, Bar[T]): ...  # error: [inconsistent-mro]
```

## Classes that inherit from themselves

These are invalid, but we need to be able to handle them gracefully without panicking.

```pyi
from ty_extensions import reveal_mro

class Foo(Foo): ...  # error: [cyclic-class-definition]

reveal_type(Foo)  # revealed: <class 'Foo'>
reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)

class Bar: ...
class Baz: ...
class Boz(Bar, Baz, Boz): ...  # error: [cyclic-class-definition]

reveal_type(Boz)  # revealed: <class 'Boz'>
reveal_mro(Boz)  # revealed: (<class 'Boz'>, Unknown, <class 'object'>)
```

## Classes with indirect cycles in their MROs

These are similarly unlikely, but we still shouldn't crash:

```pyi
from ty_extensions import reveal_mro

class Foo(Bar): ...  # error: [cyclic-class-definition]
class Bar(Baz): ...  # error: [cyclic-class-definition]
class Baz(Foo): ...  # error: [cyclic-class-definition]

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)
reveal_mro(Bar)  # revealed: (<class 'Bar'>, Unknown, <class 'object'>)
reveal_mro(Baz)  # revealed: (<class 'Baz'>, Unknown, <class 'object'>)
```

## Classes with cycles in their MROs, and multiple inheritance

```pyi
from ty_extensions import reveal_mro

class Spam: ...
class Foo(Bar): ...  # error: [cyclic-class-definition]
class Bar(Baz): ...  # error: [cyclic-class-definition]
class Baz(Foo, Spam): ...  # error: [cyclic-class-definition]

reveal_mro(Foo)  # revealed: (<class 'Foo'>, Unknown, <class 'object'>)
reveal_mro(Bar)  # revealed: (<class 'Bar'>, Unknown, <class 'object'>)
reveal_mro(Baz)  # revealed: (<class 'Baz'>, Unknown, <class 'object'>)
```

## Classes with cycles in their MRO, and a sub-graph

```pyi
from ty_extensions import reveal_mro

class FooCycle(BarCycle): ...  # error: [cyclic-class-definition]
class Foo: ...
class BarCycle(FooCycle): ...  # error: [cyclic-class-definition]
class Bar(Foo): ...

# Avoid emitting the errors for these. The classes have cyclic superclasses,
# but are not themselves cyclic...
class Baz(Bar, BarCycle): ...
class Spam(Baz): ...

reveal_mro(FooCycle)  # revealed: (<class 'FooCycle'>, Unknown, <class 'object'>)
reveal_mro(BarCycle)  # revealed: (<class 'BarCycle'>, Unknown, <class 'object'>)
reveal_mro(Baz)  # revealed: (<class 'Baz'>, Unknown, <class 'object'>)
reveal_mro(Spam)  # revealed: (<class 'Spam'>, Unknown, <class 'object'>)
```

## Other classes with possible cycles

```toml
[environment]
python-version = "3.13"
```

```pyi
from ty_extensions import reveal_mro

class C(C.a): ...
reveal_type(C.__class__)  # revealed: <class 'type'>
reveal_mro(C)  # revealed: (<class 'C'>, Unknown, <class 'object'>)

class D(D.a):
    a: D
reveal_type(D.__class__)  # revealed: <class 'type'>
reveal_mro(D)  # revealed: (<class 'D'>, Unknown, <class 'object'>)

class E[T](E.a): ...
reveal_type(E.__class__)  # revealed: <class 'type'>
reveal_mro(E)  # revealed: (<class 'E[Unknown]'>, Unknown, typing.Generic, <class 'object'>)

class F[T](F(), F): ...  # error: [cyclic-class-definition]
reveal_type(F.__class__)  # revealed: type[Unknown]
reveal_mro(F)  # revealed: (<class 'F[Unknown]'>, Unknown, <class 'object'>)
```

## `builtins.NotImplemented`

Typeshed tells us that `NotImplementedType` inherits from `Any`, but that causes more problems for
us than it fixes. We override typeshed here so that we understand `NotImplementedType` as inheriting
directly from `object` (as it does at runtime).

```py
import types
from ty_extensions import reveal_mro

reveal_mro(types.NotImplementedType)  # revealed: (<class 'NotImplementedType'>, <class 'object'>)
reveal_mro(type(NotImplemented))  # revealed: (<class 'NotImplementedType'>, <class 'object'>)
```
