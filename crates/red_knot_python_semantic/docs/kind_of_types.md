# Kinds of types

This document provides definitions for various kinds of types,
with examples of Python types that satisfy these definitions.

It doesn't attempt to be exhaustive.
Only definitions that are useful for red-knot developers should be added to this document.

## Singleton types

A singleton type is a type for which it is known that there is
(and can only ever be) exactly one known inhabitant of the type at runtime.
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

All singleton types are also sealed types, and closed types; nearly all are single-value types.
(See below for definitions of these other concepts.)

The number of singleton types in Python is theoretically infinite
(for any given theoretical runtime object `x`, you could theorise a singleton type `Y` for which
the sole inhabitant of `Y` would be the runtime object `x`). However, only a small fraction of
the set of possible singleton types are recognised as being singleton types by red-knot's model.

## Sealed types

A sealed type is a type for which it is known
that there is only a finite and pre-determined set of inhabitants at runtime.
It follows from this that for every sealed type,
there is also only a finite and pre-determined set of *subtypes* of that type.

All singleton types are sealed types; however, not all sealed types are singleton types.

Examples of sealed types (other than the singleton types listed above) are:

- `bool`:

    - The only inhabitants of `bool` at runtime are the constants `True` and `False`.
    - The only proper subtypes[^2] of `bool` are `Literal[True]`, `Literal[False]`, and `Never`;
        their union is exactly equal to `bool`.

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

    - The only proper subtypes of `Foo` are:

        - `Literal[Foo.X]`
        - `Literal[Foo.Y]`
        - `Literal[Foo.Z]`
        - `Literal[Foo.X, Foo.Y]`
        - `Literal[Foo.X, Foo.Z]`
        - `Literal[Foo.Y, Foo.Z]`
        - `Never`

        The union of the proper subtypes of `Foo` (`Literal[Foo.X, Foo.Y, Foo.Z] | Never`)
        is exactly equal to the type `Foo`.

Because a sealed type is equivalent to the union of all of its proper subtypes,
for any given sealed type `X` where the only proper subtypes of `X` are `A`, `B` and `C`,
`X & ~A == B | C`. To give a Python example:

```py
from enum import Enum, auto

class X(Enum):
    A = auto()
    B = auto()
    C = auto()

def f(var: X):
    ...  # the type of `var` here is `X` (which is equivalent to `Literal[X.A, X.B, X.C]`)

    if var is X.A:
        ...  # var is narrowed to `Literal[X.A]`
    else:
        ...  # var is narrowed to `Literal[X.B, X.C]`
```

## Single-value types

For a given type `T`, `T` can be said to be a single-value type if there exists an
[equivalence relation](https://en.wikipedia.org/wiki/Equivalence_relation)
between all inhabitants of the type and the equivalence relation is satisfied between
all inhabitants of the type. For a Python type to be categorised as a single-value type,
all inhabitants of the type must be instances of the same runtime class,
and the class in question must have `__eq__` and `__ne__` methods
such that the equality relation between instances of the class
is reflexive, symmetric, and transitive.

Nearly all singleton types are single-value types, but the reverse is not true.
Many single-value types exists that are not singleton types: examples include
`Literal[123456]`, `Literal[b"foo"]`, `tuple[Literal[True], Literal[False]]`, and `Literal["foo"]`.
All runtime inhabitants of the `Literal["foo"]` type are equal to the string `"foo"`.
However, they are not necessarily the *same* object in terms of identity;
multiple `str` instances equal to `"foo"` can exist at runtime at different memory addresses.[^1]
This means that it is not safe to narrow a single-value type by identity unless it is also known
that the type is a singleton type.

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
        return True
```

## Closed types

A closed type is a type for which it is known that the type has no subtypes
except for the type itself and `Never`; it is "closed for extension".
For a given closed type `X`, the only subtypes of `X` are `X` and `Never`;
the only *proper* subtype of `X` is `Never`.

All singleton types and single-value types are closed types.
However, not all closed types are singleton types or single-value types.

Closed types are often associated with the `@final` decorator.
Decorating a class with `@final` indicates to the type checker
that the class should never be subclassed; the type checker will emit a diagnostic
if it sees it being subclassed, and is free to consider the class as closed for extension.
As such, the following `@final` class `Spam` creates a corresponding closed type `Spam`:

```py
from typing import final

@final
class Spam: ...
```

However, runtime subclassability does not correspond exactly to whether a type is closed or not.
For example, the `bool` *class* is (correctly) decorated with `@final` in typeshed's stubs,
indicating that it cannot be subclassed at runtime. However, the `bool` *type*
has two proper subtypes other than `Never`: `Literal[True]` and `Literal[False]`.
As such, although all its subtypes are closed, `bool` itself cannot be considered a closed type.
Similarly, attempting to subclass the `Eggs` class in the following example
will lead to an exception at runtime, and type checkers should treat all
enum classes as implicitly `@final`. Nonetheless, the `Eggs` *type* cannot be considered closed,
as it has three proper subtypes other than `Never`:
`Literal[Eggs.A]`, `Literal[Eggs.B]` and `Literal[Eggs.C]`.

```py
from enum import Enum, auto

class Eggs(Enum):
    A = auto()
    B = auto()
    C = auto()
```

[^2]: For a given type `X`, a "proper subtype" of `X` is defined
    as a type that is strictly narrower than `X`. The set of proper subtypes of `X` includes
    all of `X`'s subtypes *except* for `X` itself.

[^1]: Whether or not different inhabitants of a given single-value type *actually* exist
    at different memory addresses is an implementation detail at runtime
    which cannot be determined or relied upon by a static type checker.
    It may vary according to the specific value in question,
    and/or whether the user is running CPython, PyPy, GraalPy, or IronPython.
