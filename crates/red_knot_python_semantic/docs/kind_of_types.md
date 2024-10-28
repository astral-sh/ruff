# Kinds of types

This document provides definitions for various kinds of types and classes,
with examples of Python types and classes that satisfy these definitions.

It doesn't attempt to be exhaustive.
Definitions should only be added to this document if they are:

1. Not covered by the ["Type system concepts" section of the Python typing spec](https://typing.readthedocs.io/en/latest/spec/concepts.html)
1. Definitions that would be useful for developers working on red-knot.

## Types

A static Python type represents a set of possible values at runtime.
The number of ways in which possible Python objects could be categorised into sets and subsets is infinite;
thus, there is an infinite number of possible types in Python.

The [typing spec](https://typing.readthedocs.io/en/latest/spec/) specifies various kinds of types
that all Python type checkers should support,
The desirable type checker behavior has been standardised to some degree for these specified types,
but the spec is by no means exhaustive.
There are more kinds of types in Python than are included in the spec,
and many kinds of types that are left unspecified must be understood by a type checker
in order for the type checker to accurately and precisely model Python's runtime semantics.

Some types in Python have a known size.
Most, however, have an infinite number of possible inhabitants and subtypes.
For any type `T`, the union of all of the proper subtypes[^2] of `T` is exactly equal to `T`.

## Singleton types

A singleton type is a type for which it is known that there is
(and can only ever be) exactly one inhabitant of the type at runtime:
a set of runtime values with size exactly 1.
For any singleton type in Python, the type's sole inhabitant will always exist
at the same memory address for the entire duration of a Python program.

Examples of singleton types in red-knot's model of Python include:

- `types.EllipsisType` (the sole inhabitant is `builtins.Ellipsis`;
    the constructor always returns the same object):

    ```pycon
    >>> from types import EllipsisType
    >>> EllipsisType() is EllipsisType() is ... is Ellipsis
    True
    >>> id(EllipsisType()) == id(...)
    True
    ```

- `types.NotImplementedType`
    (in the same way as `types.EllipsisType` and `builtins.Ellipsis`,
    the sole inhabitant is `builtins.NotImplemented`).

- `None` (which can also be spelled as `Literal[None]`).
    The sole inhabitant of the type is the runtime constant `None` itself.

- `Literal[True]`: the sole inhabitant of the type is the constant `True`.

- `Literal[False]`: the sole inhabitant of the type is the constant `False`.

- `Literal[E.A]`, `Literal[E.B]` or `Literal[E.C]` for the following enum `E`
    (the sole inhabitant of `Literal[E.A]` is the enum member `E.A`):

    ```py
    from enum import Enum, auto

    class E(Enum):
        A = auto()
        B = auto()
        C = auto()
    ```

- A "literal class type": a type representing a single known class (and none of its possible subclasses).

- A "literal function type": a type that represents a single known function
    (and excludes all other functions, even if they have the same signature).

- A "literal module type": a type representing a single known module.

Since it is known that all inhabitants of a given singleton type share the same memory address,
singleton types in unions can be safely narrowed by identity,
using [the operators `is` and `is not`](https://snarky.ca/unravelling-is-and-is-not/):

```py
def f(x: str | None):
    if x is None:
        ...  # x can be narrowed to `str`
    else:
        ...  # x can be narrowed to `None`
```

All Python singleton types are also sealed types and final types; nearly all are single-value types.
(See below for definitions of these other concepts.)

The number of singleton types in Python is theoretically infinite
(for any given theoretical runtime object `x`, you could theorise a singleton type `Y` for which
the sole inhabitant of `Y` would be the runtime object `x`). However, only a small fraction of
the set of possible singleton types are understood by red-knot's model.

## Sealed types

A sealed type is a type for which it is known
that there is only a finite and pre-determined set of inhabitants at runtime.
It follows from this that for every sealed type,
there is also only a finite and pre-determined set of *subtypes* of that type.

All singleton types are sealed types; however, not all sealed types are singleton types.

Examples of sealed types (other than the singleton types listed above) are:

- `bool`:

    - The only inhabitants of `bool` at runtime are the constants `True` and `False`.
    - The only proper subtypes of `bool` are `Literal[True]`, `Literal[False]`, and `Never`.

- Enums: consider the following enum class:

    ```py
    from enum import Enum, auto

    class Foo(Enum):
        X = auto()
        Y = auto()
        Z = auto()
    ```

    - The only inhabitants of the `Foo` type at runtime are the enum `Foo`'s members:
        `Foo.X`, `Foo.Y` and `Foo.Z`.

    - The only proper subtypes of `Foo` are `Literal[Foo.X]`, `Literal[Foo.Y]`, `Literal[Foo.Z]`,
        `Literal[Foo.X, Foo.Y]`, `Literal[Foo.X, Foo.Z]`, `Literal[Foo.Y, Foo.Z]`, and `Never`.

For a given sealed type `X` where the only proper subtypes of `X` are `A`, `B` and `C`,
`X & ~A == B | C`.

Due to the enumerability of a sealed type's inhabitants,
a Python sealed type can be narrowed using identity:

```py
from enum import Enum, auto

class Colors(Enum):
    RED = auto()
    BLUE = auto()
    YELLOW = auto()

def f(var: X):
    ...  # the type of `var` here is `X` (which is equivalent to `Literal[Colors.RED, Colors.BLUE, Colors.YELLOW]`)

    if var is Colors.RED:
        ...  # var is narrowed to `Literal[Colors.RED]`
    else:
        ...  # var is narrowed to `Literal[Colors.BLUE, Colors.YELLOW]`
```

## Single-value types

For a given type `T`, `T` can be said to be a single-value type if there exists an
[equivalence relation](https://en.wikipedia.org/wiki/Equivalence_relation)
between all inhabitants of the type and the equivalence relation is satisfied between
all inhabitants of the type.

For a Python type `T` to be categorised as a single-value type:

- All inhabitants of `T` must be instances of the same runtime class `U`
- `U` must have `__eq__` and `__ne__` methods such that the equality relation between instances of `U`
    is reflexive, symmetric, and transitive (satisfying the definition of
    an equivalence relation.)
- For any two inhabitants of `T` named `x` and `y`, `x == y`.

Nearly all singleton types are single-value types, but the reverse is not true.
Many single-value types exist that are not singleton types: examples include
`Literal[123456]`, `Literal[b"foo"]`, `tuple[Literal[True], Literal[False]]`, and `Literal["foo"]`.
All runtime inhabitants of the `Literal["foo"]` type are equal to the string `"foo"`.
However, they are not necessarily the *same* object;
multiple `str` instances equal to `"foo"` can exist at runtime at different memory addresses.[^1]
This means that it is not safe to narrow a single-value type by identity unless it is also known
that the type is a sealed type.

Single-value types in unions can be safely narrowed using inequality (but not using equality):

```py
from typing import Literal

def f(x: str | Literal[1]):
    if x != 1:
        ...  # `x` can be narrowed to `str` if not equal to `1`
    else:
        ...  # type of `x` is still `str | Literal[1]`
             # (`x` could be an instance of a `str` subclass that overrides `__eq__`
             # to compare equal with `1`)

    if x == 1:
        ...  # type of `x` is still `str | Literal[1]`
             # (`x` could be an instance of a `str` subclass that overrides `__eq__`
             # to compare equal with `1`)
    else:
        ...  # `x` can be narrowed to `str` if not equal to `1`
```

An example of a singleton type that is not a single-value type
would be `Literal[Ham.A]` for the following `Ham` enum:
because the `Ham` class overrides `__eq__`, we can no longer say for sure
that the `Ham` type can be safely narrowed using inequality as in the above example
(but we *can* still narrow the `Ham` type using identity, as with all singleton types):

```py
from enum import Enum, auto

class Ham(Enum):
    A = auto()
    B = auto()
    C = auto()

    def __eq__(self, other):
        return False
```

## Final classes

A "final class" in Python is a class that can be statically inferred as being unsubclassable.
Most final classes are decorated with [`@typing.final`](https://docs.python.org/3/library/typing.html#typing.final),
a special decorator that indicates to the type checker that the tool should assume
the class cannot be subclassed, and that the tool should emit an error
if it detects an attempt to do so. Not all final classes are decorated with `@final`, however.
The most common example of this is enum classes. Any enum class that has at least one member is considered
implicitly final, as attempting to subclass such a class will fail at runtime.

For any two classes `X` and `Y`, if `X` is final and `X` is not a subclass of `Y`,
the intersection of the types `X & Y` is equivalent to `Never`, the empty set.
This therefore means that for a type defined as being "all instances of the final class `X`",
there are fewer ways of subtyping such a type than for most types in Python.
However, final class types can still have non-`Never` proper subtypes.
Enums again provide a good example here: `Literal[Animals.LION]` is a proper subtype
of the `Animals` type here, despite the fact that the `Animals` class cannot be subclassed:

```py
from enum import Enum, auto

class Animals(Enum):
    LION = auto()
    LEOPARD = auto()
    CHEETAH = auto()
```

Many singleton and sealed types are associated with final classes: the classes
`types.EllipsisType`, `types.NotImplementedType`, `types.NoneType`, `bool`,
and all enum classes are final classes. However, there are also many singleton
and sealed types (some representable, but some merely theoretical)
that cannot be defined as "all instances of a specific final class `X`".
In the above enum, `Literal[Animals.LION]` is a singleton type and a sealed type,
but would not fit this definition.

[^2]: For a given type `X`, a "proper subtype" of `X` is defined
    as a type that is strictly narrower than `X`. The set of proper subtypes of `X` includes
    all of `X`'s subtypes *except* for `X` itself.

[^1]: Whether or not different inhabitants of a given single-value type *actually* exist
    at different memory addresses is an implementation detail at runtime
    which cannot be determined or relied upon by a static type checker.
    It may vary according to the specific value in question,
    and/or whether the user is running CPython, PyPy, GraalPy, or IronPython.
