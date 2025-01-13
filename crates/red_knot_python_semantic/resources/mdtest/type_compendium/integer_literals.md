# Integer `Literal`s

An integer literal type represents the set of all integer objects with one specific value. For
example, the type `Literal[54165]` represents the set of all integer objects with the value `54165`.

## Integer `Literal`s are not singleton types

This does not necessarily mean that the type is a singleton type, i.e., a type with only one
inhabitant. The reason for this is that there might be multiple Python runtime objects (at different
memory locations) that all represent the same integer value. For example, the following code snippet
may print `False`.

```py path=interned.py
x = 54165
y = 54165

print(x is y)
```

In practice, on CPython 3.13.0, this program prints `True` when executed as a script, but `False`
when executed in the REPL.

Since this is an implementation detail of the Python runtime, we model all integer literals as
non-singleton types:

```py
from knot_extensions import static_assert, is_singleton
from typing import Literal

static_assert(not is_singleton(Literal[0]))
static_assert(not is_singleton(Literal[1]))
static_assert(not is_singleton(Literal[54165]))
```

This has implications for type-narrowing. For example, you can not use the `is not` operator to
check whether a variable has a specific integer literal type, but this is not a recommended practice
anyway.

```py path=type_narrowing.py
def f(x: int):
    if x is 54165:
        # This works, because if `x` is the same object as that left-hand-side literal, then it
        # must have the same value.
        reveal_type(x)  # revealed: Literal[54165]

    if x is not 54165:
        # But here, we can not narrow the type (to `int & ~Literal[54165]`), because `x` might also
        # have the value `54165`, but a different object identity.
        reveal_type(x)  # revealed: int
```

## Integer `Literal`s are single-valued types

There is a slightly weaker property that integer literals have. They are single-valued types, which
means that all objects of the type have the same value, i.e. they compare equal to each other:

```py
from knot_extensions import static_assert, is_single_valued

static_assert(is_single_valued(Literal[0]))
static_assert(is_single_valued(Literal[1]))
static_assert(is_single_valued(Literal[54165]))
```

And this can be used for type-narrowing using not-equal comparisons:

```py path=type_narrowing.py
def f(x: int):
    if x == 54165:
        # The reason that no narrowing occurs here is that there might be subclasses of `int`
        # that override `__eq__`. This is not specific to integer literals though, and generally
        # applies to `==` comparisons.
        reveal_type(x)  # revealed: int

    if x != 54165:
        reveal_type(x)  # revealed: int & ~Literal[54165]
```

## Subtyping relationships

### Subtypes of `int`

All integer literals are subtypes of `int`:

```py
from knot_extensions import static_assert, is_subtype_of

static_assert(is_subtype_of(Literal[0], int))
static_assert(is_subtype_of(Literal[1], int))
static_assert(is_subtype_of(Literal[54165], int))
```

It is tempting to think that `int` is equivalent to the union of all integer literals,
`… | Literal[-1] | Literal[0] | Literal[1] | …`, but this is not the case. `True` and `False` are
also inhabitants of the `int` type, but they are not inhabitants of any integer literal type:

```py path=true_and_false.py
from knot_extensions import static_assert, is_subtype_of

static_assert(is_subtype_of(Literal[True], int))
static_assert(is_subtype_of(Literal[False], int))

static_assert(not is_subtype_of(Literal[True], Literal[1]))
static_assert(not is_subtype_of(Literal[False], Literal[0]))
```

### No subtypes of `float` and `complex`

Integer literals are _not_ subtypes of `float`, but the typing spec describes a
[special cases for `float` and `complex`] which accepts integer literals in places where a `float`
or `complex` is expected.

```py
from knot_extensions import static_assert, is_subtype_of

# Not subtypes of `float`
static_assert(not is_subtype_of(Literal[0], float) and not is_subtype_of(Literal[0], complex))
static_assert(not is_subtype_of(Literal[1], float) and not is_subtype_of(Literal[1], complex))
static_assert(not is_subtype_of(Literal[54165], float) and not is_subtype_of(Literal[54165], complex))

# TODO: This should not raise errors, see https://github.com/astral-sh/ruff/issues/14932
def f(
    # error: [invalid-parameter-default]
    x: float = 0,
    # error: [invalid-parameter-default]
    y: complex = 1,
):
    pass
```

### Subtypes of integer `Literal`s?

The only spellable subtypes of an integer literal type are the type itself and `Never`:

```py
from knot_extensions import static_assert, is_subtype_of
from typing_extensions import Never

static_assert(is_subtype_of(Literal[54165], Literal[54165]))
static_assert(is_subtype_of(Never, Literal[54165]))
```

## Disjointness of integer `Literal`s

Two integer literal types `Literal[a]` and `Literal[b]` are disjoint if `a != b`:

```py
from knot_extensions import static_assert, is_disjoint_from

static_assert(is_disjoint_from(Literal[0], Literal[1]))
static_assert(is_disjoint_from(Literal[0], Literal[54165]))

static_assert(not is_disjoint_from(Literal[0], Literal[0]))
static_assert(not is_disjoint_from(Literal[54165], Literal[54165]))
```

## Integer literal math

We support a whole range of arithmetic operations on integer literal types. For example:

```py
from knot_extensions import static_assert

static_assert(3**2 + 4**2 == 5**2)
```

## Truthiness

Integer literals are always-truthy, except for `0`, which is always-falsy:

```py
from knot_extensions import static_assert

static_assert(-54165)
static_assert(-1)
static_assert(not 0)
static_assert(1)
static_assert(54165)
```

This can be used for type-narrowing:

```py path=type_narrowing.py
from knot_extensions import static_assert
from typing_extensions import Literal, assert_type

def f(x: Literal[0, 1, 54365]):
    if x:
        assert_type(x, Literal[1, 54365])
    else:
        assert_type(x, Literal[0])
```

[special cases for `float` and `complex`]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
