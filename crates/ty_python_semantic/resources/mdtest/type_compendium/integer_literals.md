# Integer `Literal`s

An integer literal type represents the set of all integer objects with one specific value. For
example, the type `Literal[54165]` represents the set of all integer objects with the value `54165`.

## Integer `Literal`s are not singleton types

This does not necessarily mean that the type is a singleton type, i.e., a type with only one
inhabitant. The reason for this is that there might be multiple Python runtime objects (at different
memory locations) that all represent the same integer value. For example, the following code snippet
may print `False`.

```py
x = 54165
y = 54165

print(x is y)
```

In practice, on CPython 3.13.0, this program prints `True` when executed as a script, but `False`
when executed in the REPL.

Since this is an implementation detail of the Python runtime, we model all integer literals as
non-singleton types:

```py
from ty_extensions import static_assert, is_singleton
from typing import Literal

static_assert(not is_singleton(Literal[0]))
static_assert(not is_singleton(Literal[1]))
static_assert(not is_singleton(Literal[54165]))
```

This has implications for type-narrowing. For example, you can not use the `is not` operator to
check whether a variable has a specific integer literal type, but this is not a recommended practice
anyway.

```py
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
from ty_extensions import static_assert, is_single_valued
from typing import Literal

static_assert(is_single_valued(Literal[0]))
static_assert(is_single_valued(Literal[1]))
static_assert(is_single_valued(Literal[54165]))
```

And this can be used for type-narrowing using not-equal comparisons:

```py
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
from ty_extensions import static_assert, is_subtype_of
from typing import Literal

static_assert(is_subtype_of(Literal[0], int))
static_assert(is_subtype_of(Literal[1], int))
static_assert(is_subtype_of(Literal[54165], int))
```

It is tempting to think that `int` is equivalent to the union of all integer literals,
`… | Literal[-1] | Literal[0] | Literal[1] | …`, but this is not the case. `True` and `False` are
also inhabitants of the `int` type, but they are not inhabitants of any integer literal type:

```py
static_assert(is_subtype_of(Literal[True], int))
static_assert(is_subtype_of(Literal[False], int))

static_assert(not is_subtype_of(Literal[True], Literal[1]))
static_assert(not is_subtype_of(Literal[False], Literal[0]))
```

Also, `int` can be subclassed, and instances of that subclass are also subtypes of `int`:

```py
class CustomInt(int):
    pass

static_assert(is_subtype_of(CustomInt, int))
```

### No subtypes of `float` and `complex`

```toml
[environment]
python-version = "3.12"
```

Integer literals are _not_ subtypes of `float`, but the typing spec describes a special case for
[`float` and `complex`] which accepts integers (and therefore also integer literals) in places where
a `float` or `complex` is expected. We use the types `JustFloat` and `JustComplex` below, because ty
recognizes an annotation of `float` as `int | float` to support that typing system special case.

```py
from ty_extensions import static_assert, is_subtype_of, JustFloat, JustComplex
from typing import Literal

# Not subtypes of `float` and `complex`
static_assert(not is_subtype_of(Literal[0], JustFloat) and not is_subtype_of(Literal[0], JustComplex))
static_assert(not is_subtype_of(Literal[1], JustFloat) and not is_subtype_of(Literal[1], JustComplex))
static_assert(not is_subtype_of(Literal[54165], JustFloat) and not is_subtype_of(Literal[54165], JustComplex))
```

The typing system special case can be seen in the following example:

```py
a: JustFloat = 1  # error: [invalid-assignment]
b: JustComplex = 1  # error: [invalid-assignment]

x: float = 1
y: complex = 1
```

### Subtypes of integer `Literal`s?

The only subtypes of an integer literal type _that can be named_ are the type itself and `Never`:

```py
from ty_extensions import static_assert, is_subtype_of
from typing_extensions import Never, Literal

static_assert(is_subtype_of(Literal[54165], Literal[54165]))
static_assert(is_subtype_of(Never, Literal[54165]))
```

## Disjointness of integer `Literal`s

Two integer literal types `Literal[a]` and `Literal[b]` are disjoint if `a != b`:

```py
from ty_extensions import static_assert, is_disjoint_from
from typing import Literal

static_assert(is_disjoint_from(Literal[0], Literal[1]))
static_assert(is_disjoint_from(Literal[0], Literal[54165]))

static_assert(not is_disjoint_from(Literal[0], Literal[0]))
static_assert(not is_disjoint_from(Literal[54165], Literal[54165]))
```

## Integer literal math

```toml
[environment]
python-version = "3.12"
```

We support a whole range of arithmetic operations on integer literal types. For example, we can
statically verify that (3, 4, 5) is a Pythagorean triple:

```py
from ty_extensions import static_assert

static_assert(3**2 + 4**2 == 5**2)
```

Using unions of integer literals, we can even use this to solve equations over a finite domain
(determine whether there is a solution or not):

```py
from typing import Literal, assert_type

type Nat = Literal[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]

def pythagorean_triples(a: Nat, b: Nat, c: Nat):
    # Answer is `bool`, because solutions do exist (3² + 4² = 5²)
    assert_type(a**2 + b**2 == c**2, bool)

def fermats_last_theorem(a: Nat, b: Nat, c: Nat):
    # Answer is `Literal[False]`, because no solutions exist
    assert_type(a**3 + b**3 == c**3, Literal[False])
```

## Truthiness

Integer literals are always-truthy, except for `0`, which is always-falsy:

```py
from ty_extensions import static_assert

static_assert(-54165)
static_assert(-1)
static_assert(not 0)
static_assert(1)
static_assert(54165)
```

This can be used for type-narrowing:

```py
from typing_extensions import Literal, assert_type

def f(x: Literal[0, 1, 54365]):
    if x:
        assert_type(x, Literal[1, 54365])
    else:
        assert_type(x, Literal[0])
```

[`float` and `complex`]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
