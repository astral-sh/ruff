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

- `types.EllipsisType` (the sole inhabitant is `builtins.Ellipsis`).
- `types.NotImplementedType` (the sole inhabitant is `builtins.NotImplemented`).
- `None` (which can also be spelled as `Literal[None]`).
    The sole inhabitant of the type is the runtime constant `None` itself.
- `Literal[True]`: the sole inhabitant of the type is the constant `True`.
- `Literal[False]`: the sole inhabitant of the type is the constant `False`.
- `Literal[E.A]`, where `E` is an enum class which has a member `A` (the sole inhabitant is `E.A`).
- A "literal class type": a type representing a single known class (and none of its possible subclasses).
- A "literal function type": a type representing a single known function
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

All singleton types are also sealed types, closed types and single-value types
(see below for definitions).

## Sealed types

A sealed type is a type for which it is known
that there is only a finite and pre-determined set of inhabitants at runtime.
It follows from this that for every sealed type,
there is also only a finite and pre-determined set of *subtypes* of that type.

A sealed type in Python has the property
that its proper subtypes[^2] are all disjunct from one another,
and the union of all the proper subtypes of a sealed type is exactly equal to the sealed type.

All singleton types are sealed types; however, not all sealed types are singleton types.

Examples of sealed types (other than the singleton types listed above) are:

- `bool`:

    - The only inhabitants of `bool` at runtime are the constants `True` and `False`.
    - The only proper subtypes of `bool` are `Literal[True]`, `Literal[False]`, and `Never`.
        `Literal[True]`, `Literal[False]` and `Never` are all disjunct types,
        and their union is exactly equal to `bool`.

- Enums: consider the following enum class:

    ```py
    from enum import Enum, auto

    class Foo(Enum):
        X = auto()
        Y = auto()
        Z = auto()
    ```

    - The only inhabitants of the `Foo` type at runtime are the enum `Foo`'s members:
        `Foo.X` and `Foo.Y`
    - The only proper subtypes of `Foo` are `Literal[Foo.X]`, `Literal[Foo.Y]`, `Literal[Foo.Z]`, and `Never`.
        `Literal[Foo.X]`, `Literal[Foo.Y]`, `Literal[Foo.Z]` and `Never` are all disjunct,
        and the union `Literal[Foo.X, Foo.Y, Foo.Z] | Never` is exactly equal to the type `Foo`.

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

A single-value type is a non-empty type for which it is known
that all inhabitants of the type are equivalent with respect to their runtime value.
All singleton types are single-value types, but not all single-value types are singleton types.

Examples of single-value types that are not singleton types
are `Literal["foo"]`, `Literal[b"foo"]`, and `Literal[123456]`.
All inhabitants of the `Literal["foo"]` type are entirely fungible and equal
(they are all equal to the string `"foo"`).
However, they are not necessarily the *same* object in terms of identity;
multiple instances of the `"foo"` string can exist at runtime at different memory addresses.[^1]
This means that it is not safe to narrow a single-value type by identity unless it is also known
that the type is a singleton type.

In order for a type to be considered a single-value type,
there must exist a reflexive, symmetric, and transitive equivalence relation
between all inhabitants of the type. It follows that for a single-value type in Python,
all inhabitants must be instances of the same runtime class,
and the class in question must have `__eq__` and `__ne__` methods
such that the equality relation between instances of the class
is reflexive, symmetric, and transitive.

Single-value types in unions can be safely narrowed using inequality (but not using equality):

```py
from typing import Literal

def f(x: str | Literal[1]):
    if x != 1:
        ...  # x can be narrowed to `str` if not equal to `1`
    else:
        ...  # type of x is still `str | Literal[1]`
             # (`x` could be an instance of a `str` subclass that overrides `__eq__`
             # to compare equal with `1`)

    if x == 1:
        ...  # type of x is still `str | Literal[1]`
             # (`x` could be an instance of a `str` subclass that overrides `__eq__`
             # to compare equal with `1`)
    else:
        ...  # x can be narrowed to `str` if not equal to `1`
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

However, runtime subclassability does not correspond exactly to whether a class is closed or not.
For example, the `bool` type is (correctly) decorated with `@final` in typeshed's stubs,
indicating that the class cannot be subclassed at runtime.
However, `bool` has two proper subtypes other than `Never`: `Literal[True]` and `Literal[False]`.
As such, although both its subtypes are closed, `bool` itself cannot be considered a closed type.
Similarly, attempting to subclass the `Eggs` enum in the following example
will lead to an exception at runtime, and type checkers should treat all
enum classes as implicitly `@final`. Nonetheless, the `Eggs` type cannot be considered closed,
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
