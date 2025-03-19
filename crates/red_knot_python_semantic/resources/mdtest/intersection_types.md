# Intersection types

## Introduction

This test suite covers certain properties of intersection types and makes sure that we can apply
various simplification strategies. We use `Intersection` (`&`) and `Not` (`~`) to construct
intersection types (note that we display negative contributions at the end; the order does not
matter):

```py
from knot_extensions import Intersection, Not

class P: ...
class Q: ...

def _(
    i1: Intersection[P, Q],
    i2: Intersection[P, Not[Q]],
    i3: Intersection[Not[P], Q],
    i4: Intersection[Not[P], Not[Q]],
) -> None:
    reveal_type(i1)  # revealed: P & Q
    reveal_type(i2)  # revealed: P & ~Q
    reveal_type(i3)  # revealed: Q & ~P
    reveal_type(i4)  # revealed: ~P & ~Q
```

## Notation

Throughout this document, we use the following types as representatives for certain equivalence
classes.

### Non-disjoint types

We use `P`, `Q`, `R`, … to denote types that are non-disjoint:

```py
from knot_extensions import static_assert, is_disjoint_from

class P: ...
class Q: ...
class R: ...

static_assert(not is_disjoint_from(P, Q))
static_assert(not is_disjoint_from(P, R))
static_assert(not is_disjoint_from(Q, R))
```

Although `P` is not a subtype of `Q` and `Q` is not a subtype of `P`, the two types are not disjoint
because it would be possible to create a class `S` that inherits from both `P` and `Q` using
multiple inheritance. An instance of `S` would be a member of the `P` type _and_ the `Q` type.

### Disjoint types

We use `Literal[1]`, `Literal[2]`, … as examples of pairwise-disjoint types, and `int` as a joint
supertype of these:

```py
from knot_extensions import static_assert, is_disjoint_from, is_subtype_of
from typing import Literal

static_assert(is_disjoint_from(Literal[1], Literal[2]))
static_assert(is_disjoint_from(Literal[1], Literal[3]))
static_assert(is_disjoint_from(Literal[2], Literal[3]))

static_assert(is_subtype_of(Literal[1], int))
static_assert(is_subtype_of(Literal[2], int))
static_assert(is_subtype_of(Literal[3], int))
```

### Subtypes

Finally, we use `A <: B <: C` and `A <: B1`, `A <: B2` to denote hierarchies of (proper) subtypes:

```py
from knot_extensions import static_assert, is_subtype_of, is_disjoint_from

class A: ...
class B(A): ...
class C(B): ...

static_assert(is_subtype_of(B, A))
static_assert(is_subtype_of(C, B))
static_assert(is_subtype_of(C, A))

static_assert(not is_subtype_of(A, B))
static_assert(not is_subtype_of(B, C))
static_assert(not is_subtype_of(A, C))

class B1(A): ...
class B2(A): ...

static_assert(is_subtype_of(B1, A))
static_assert(is_subtype_of(B2, A))

static_assert(not is_subtype_of(A, B1))
static_assert(not is_subtype_of(A, B2))

static_assert(not is_subtype_of(B1, B2))
static_assert(not is_subtype_of(B2, B1))
```

## Structural properties

This section covers structural properties of intersection types and documents some decisions on how
to represent mixtures of intersections and unions.

### Single-element intersections

If we have an intersection with a single element, we can simplify to that element. Similarly, we
show an intersection with a single negative contribution as just the negation of that element.

```py
from knot_extensions import Intersection, Not

class P: ...

def _(
    i1: Intersection[P],
    i2: Intersection[Not[P]],
) -> None:
    reveal_type(i1)  # revealed: P
    reveal_type(i2)  # revealed: ~P
```

### Flattening of nested intersections

We eagerly flatten nested intersections types.

```py
from knot_extensions import Intersection, Not

class P: ...
class Q: ...
class R: ...
class S: ...

def positive_contributions(
    i1: Intersection[P, Intersection[Q, R]],
    i2: Intersection[Intersection[P, Q], R],
) -> None:
    reveal_type(i1)  # revealed: P & Q & R
    reveal_type(i2)  # revealed: P & Q & R

def negative_contributions(
    i1: Intersection[Not[P], Intersection[Not[Q], Not[R]]],
    i2: Intersection[Intersection[Not[P], Not[Q]], Not[R]],
) -> None:
    reveal_type(i1)  # revealed: ~P & ~Q & ~R
    reveal_type(i2)  # revealed: ~P & ~Q & ~R

def mixed(
    i1: Intersection[P, Intersection[Not[Q], R]],
    i2: Intersection[Intersection[P, Not[Q]], R],
    i3: Intersection[Not[P], Intersection[Q, Not[R]]],
    i4: Intersection[Intersection[Q, Not[R]], Not[P]],
) -> None:
    reveal_type(i1)  # revealed: P & R & ~Q
    reveal_type(i2)  # revealed: P & R & ~Q
    reveal_type(i3)  # revealed: Q & ~P & ~R
    reveal_type(i4)  # revealed: Q & ~R & ~P

def multiple(
    i1: Intersection[Intersection[P, Q], Intersection[R, S]],
):
    reveal_type(i1)  # revealed: P & Q & R & S

def nested(
    i1: Intersection[Intersection[Intersection[P, Q], R], S],
    i2: Intersection[P, Intersection[Q, Intersection[R, S]]],
):
    reveal_type(i1)  # revealed: P & Q & R & S
    reveal_type(i2)  # revealed: P & Q & R & S
```

### Union of intersections

We always normalize our representation to a _union of intersections_, so when we add a _union to an
intersection_, we distribute the union over the respective elements:

```py
from knot_extensions import Intersection, Not

class P: ...
class Q: ...
class R: ...
class S: ...

def _(
    i1: Intersection[P, Q | R | S],
    i2: Intersection[P | Q | R, S],
    i3: Intersection[P | Q, R | S],
) -> None:
    reveal_type(i1)  # revealed: P & Q | P & R | P & S
    reveal_type(i2)  # revealed: P & S | Q & S | R & S
    reveal_type(i3)  # revealed: P & R | Q & R | P & S | Q & S

def simplifications_for_same_elements(
    i1: Intersection[P, Q | P],
    i2: Intersection[Q, P | Q],
    i3: Intersection[P | Q, Q | R],
    i4: Intersection[P | Q, P | Q],
    i5: Intersection[P | Q, Q | P],
) -> None:
    #   P & (Q | P)
    # = P & Q | P & P
    # = P & Q | P
    # = P
    # (because P is a supertype of P & Q)
    reveal_type(i1)  # revealed: P
    # similar here:
    reveal_type(i2)  # revealed: Q

    #   (P | Q) & (Q | R)
    # = P & Q | P & R | Q & Q | Q & R
    # = P & Q | P & R | Q | Q & R
    # = Q | P & R
    # (again, because Q is a supertype of P & Q and of Q & R)
    reveal_type(i3)  # revealed: Q | P & R

    #   (P | Q) & (P | Q)
    # = P & P | P & Q | Q & P | Q & Q
    # = P | P & Q | Q
    # = P | Q
    reveal_type(i4)  # revealed: P | Q
```

### Negation distributes over union

Distribution also applies to a negation operation. This is a manifestation of one of
[De Morgan's laws], namely `~(P | Q) = ~P & ~Q`:

```py
from knot_extensions import Not
from typing import Literal

class P: ...
class Q: ...
class R: ...

def _(i1: Not[P | Q], i2: Not[P | Q | R]) -> None:
    reveal_type(i1)  # revealed: ~P & ~Q
    reveal_type(i2)  # revealed: ~P & ~Q & ~R

def example_literals(i: Not[Literal[1, 2]]) -> None:
    reveal_type(i)  # revealed: ~Literal[1] & ~Literal[2]
```

### Negation of intersections

The other of [De Morgan's laws], `~(P & Q) = ~P | ~Q`, also holds:

```py
from knot_extensions import Intersection, Not

class P: ...
class Q: ...
class R: ...

def _(
    i1: Not[Intersection[P, Q]],
    i2: Not[Intersection[P, Q, R]],
) -> None:
    reveal_type(i1)  # revealed: ~P | ~Q
    reveal_type(i2)  # revealed: ~P | ~Q | ~R
```

### `Never` is dual to `object`

`Never` represents the empty set of values, while `object` represents the set of all values, so
`~Never` is equivalent to `object`, and `~object` is equivalent to `Never`. This is a manifestation
of the [complement laws] of set theory.

```py
from knot_extensions import Intersection, Not
from typing_extensions import Never

def _(
    not_never: Not[Never],
    not_object: Not[object],
) -> None:
    reveal_type(not_never)  # revealed: object
    reveal_type(not_object)  # revealed: Never
```

### `object & ~T` is equivalent to `~T`

A second consequence of the fact that `object` is the top type is that `object` is always redundant
in intersections, and can be eagerly simplified out. `object & P` is equivalent to `P`;
`object & ~P` is equivalent to `~P` for any type `P`.

```py
from knot_extensions import Intersection, Not, is_equivalent_to, static_assert

class P: ...

static_assert(is_equivalent_to(Intersection[object, P], P))
static_assert(is_equivalent_to(Intersection[object, Not[P]], Not[P]))
```

### Intersection of a type and its negation

Continuing with more [complement laws], if we see both `P` and `~P` in an intersection, we can
simplify to `Never`, even in the presence of other types:

```py
from knot_extensions import Intersection, Not
from typing import Any

class P: ...
class Q: ...

def _(
    i1: Intersection[P, Not[P]],
    i2: Intersection[Not[P], P],
    i3: Intersection[P, Q, Not[P]],
    i4: Intersection[Not[P], Q, P],
    i5: Intersection[P, Any, Not[P]],
    i6: Intersection[Not[P], Any, P],
) -> None:
    reveal_type(i1)  # revealed: Never
    reveal_type(i2)  # revealed: Never
    reveal_type(i3)  # revealed: Never
    reveal_type(i4)  # revealed: Never
    reveal_type(i5)  # revealed: Never
    reveal_type(i6)  # revealed: Never
```

### Union of a type and its negation

Similarly, if we have both `P` and `~P` in a _union_, we can simplify that to `object`.

```py
from knot_extensions import Intersection, Not

class P: ...
class Q: ...

def _(
    i1: P | Not[P],
    i2: Not[P] | P,
    i3: P | Q | Not[P],
    i4: Not[P] | Q | P,
) -> None:
    reveal_type(i1)  # revealed: object
    reveal_type(i2)  # revealed: object
    reveal_type(i3)  # revealed: object
    reveal_type(i4)  # revealed: object
```

### Negation is an involution

The final of the [complement laws] states that negating twice is equivalent to not negating at all:

```py
from knot_extensions import Not

class P: ...

def _(
    i1: Not[P],
    i2: Not[Not[P]],
    i3: Not[Not[Not[P]]],
    i4: Not[Not[Not[Not[P]]]],
) -> None:
    reveal_type(i1)  # revealed: ~P
    reveal_type(i2)  # revealed: P
    reveal_type(i3)  # revealed: ~P
    reveal_type(i4)  # revealed: P
```

## Simplification strategies

In this section, we present various simplification strategies that go beyond the structure of the
representation.

### `Never` in intersections

If we intersect with `Never`, we can simplify the whole intersection to `Never`, even if there are
dynamic types involved:

```py
from knot_extensions import Intersection, Not
from typing_extensions import Never, Any

class P: ...
class Q: ...

def _(
    i1: Intersection[P, Never],
    i2: Intersection[Never, P],
    i3: Intersection[Any, Never],
    i4: Intersection[Never, Not[Any]],
) -> None:
    reveal_type(i1)  # revealed: Never
    reveal_type(i2)  # revealed: Never
    reveal_type(i3)  # revealed: Never
    reveal_type(i4)  # revealed: Never
```

### Simplifications using disjointness

#### Positive contributions

If we intersect disjoint types, we can simplify to `Never`, even in the presence of other types:

```py
from knot_extensions import Intersection, Not
from typing import Literal, Any

class P: ...

def _(
    i01: Intersection[Literal[1], Literal[2]],
    i02: Intersection[Literal[2], Literal[1]],
    i03: Intersection[Literal[1], Literal[2], P],
    i04: Intersection[Literal[1], P, Literal[2]],
    i05: Intersection[P, Literal[1], Literal[2]],
    i06: Intersection[Literal[1], Literal[2], Any],
    i07: Intersection[Literal[1], Any, Literal[2]],
    i08: Intersection[Any, Literal[1], Literal[2]],
) -> None:
    reveal_type(i01)  # revealed: Never
    reveal_type(i02)  # revealed: Never
    reveal_type(i03)  # revealed: Never
    reveal_type(i04)  # revealed: Never
    reveal_type(i05)  # revealed: Never
    reveal_type(i06)  # revealed: Never
    reveal_type(i07)  # revealed: Never
    reveal_type(i08)  # revealed: Never

# `bool` is final and can not be subclassed, so `type[bool]` is equivalent to `Literal[bool]`, which
# is disjoint from `type[str]`:
def example_type_bool_type_str(
    i: Intersection[type[bool], type[str]],
) -> None:
    reveal_type(i)  # revealed: Never
```

#### Positive and negative contributions

If we intersect a type `X` with the negation `~Y` of a disjoint type `Y`, we can remove the negative
contribution `~Y`, as `~Y` must fully contain the positive contribution `X` as a subtype:

```py
from knot_extensions import Intersection, Not
from typing import Literal

def _(
    i1: Intersection[Literal[1], Not[Literal[2]]],
    i2: Intersection[Not[Literal[2]], Literal[1]],
    i3: Intersection[Literal[1], Not[Literal[2]], int],
    i4: Intersection[Literal[1], int, Not[Literal[2]]],
    i5: Intersection[int, Literal[1], Not[Literal[2]]],
) -> None:
    reveal_type(i1)  # revealed: Literal[1]
    reveal_type(i2)  # revealed: Literal[1]
    reveal_type(i3)  # revealed: Literal[1]
    reveal_type(i4)  # revealed: Literal[1]
    reveal_type(i5)  # revealed: Literal[1]

# None is disjoint from int, so this simplification applies here
def example_none(
    i1: Intersection[int, Not[None]],
    i2: Intersection[Not[None], int],
) -> None:
    reveal_type(i1)  # revealed: int
    reveal_type(i2)  # revealed: int
```

### Simplifications using subtype relationships

#### Positive type and positive subtype

Subtypes are contained within their supertypes, so we can simplify intersections by removing
superfluous supertypes:

```py
from knot_extensions import Intersection, Not
from typing import Any

class A: ...
class B(A): ...
class C(B): ...
class Unrelated: ...

def _(
    i01: Intersection[A, B],
    i02: Intersection[B, A],
    i03: Intersection[A, C],
    i04: Intersection[C, A],
    i05: Intersection[B, C],
    i06: Intersection[C, B],
    i07: Intersection[A, B, C],
    i08: Intersection[C, B, A],
    i09: Intersection[B, C, A],
    i10: Intersection[A, B, Unrelated],
    i11: Intersection[B, A, Unrelated],
    i12: Intersection[B, Unrelated, A],
    i13: Intersection[A, Unrelated, B],
    i14: Intersection[Unrelated, A, B],
    i15: Intersection[Unrelated, B, A],
    i16: Intersection[A, B, Any],
    i17: Intersection[B, A, Any],
    i18: Intersection[B, Any, A],
    i19: Intersection[A, Any, B],
    i20: Intersection[Any, A, B],
    i21: Intersection[Any, B, A],
) -> None:
    reveal_type(i01)  # revealed: B
    reveal_type(i02)  # revealed: B
    reveal_type(i03)  # revealed: C
    reveal_type(i04)  # revealed: C
    reveal_type(i05)  # revealed: C
    reveal_type(i06)  # revealed: C
    reveal_type(i07)  # revealed: C
    reveal_type(i08)  # revealed: C
    reveal_type(i09)  # revealed: C
    reveal_type(i10)  # revealed: B & Unrelated
    reveal_type(i11)  # revealed: B & Unrelated
    reveal_type(i12)  # revealed: B & Unrelated
    reveal_type(i13)  # revealed: Unrelated & B
    reveal_type(i14)  # revealed: Unrelated & B
    reveal_type(i15)  # revealed: Unrelated & B
    reveal_type(i16)  # revealed: B & Any
    reveal_type(i17)  # revealed: B & Any
    reveal_type(i18)  # revealed: B & Any
    reveal_type(i19)  # revealed: Any & B
    reveal_type(i20)  # revealed: Any & B
    reveal_type(i21)  # revealed: Any & B
```

#### Negative type and negative subtype

For negative contributions, this property is reversed. Here we can remove superfluous _subtypes_:

```py
from knot_extensions import Intersection, Not
from typing import Any

class A: ...
class B(A): ...
class C(B): ...
class Unrelated: ...

def _(
    i01: Intersection[Not[B], Not[A]],
    i02: Intersection[Not[A], Not[B]],
    i03: Intersection[Not[A], Not[C]],
    i04: Intersection[Not[C], Not[A]],
    i05: Intersection[Not[B], Not[C]],
    i06: Intersection[Not[C], Not[B]],
    i07: Intersection[Not[A], Not[B], Not[C]],
    i08: Intersection[Not[C], Not[B], Not[A]],
    i09: Intersection[Not[B], Not[C], Not[A]],
    i10: Intersection[Not[B], Not[A], Unrelated],
    i11: Intersection[Not[A], Not[B], Unrelated],
    i12: Intersection[Not[A], Unrelated, Not[B]],
    i13: Intersection[Not[B], Unrelated, Not[A]],
    i14: Intersection[Unrelated, Not[A], Not[B]],
    i15: Intersection[Unrelated, Not[B], Not[A]],
    i16: Intersection[Not[B], Not[A], Any],
    i17: Intersection[Not[A], Not[B], Any],
    i18: Intersection[Not[A], Any, Not[B]],
    i19: Intersection[Not[B], Any, Not[A]],
    i20: Intersection[Any, Not[A], Not[B]],
    i21: Intersection[Any, Not[B], Not[A]],
) -> None:
    reveal_type(i01)  # revealed: ~A
    reveal_type(i02)  # revealed: ~A
    reveal_type(i03)  # revealed: ~A
    reveal_type(i04)  # revealed: ~A
    reveal_type(i05)  # revealed: ~B
    reveal_type(i06)  # revealed: ~B
    reveal_type(i07)  # revealed: ~A
    reveal_type(i08)  # revealed: ~A
    reveal_type(i09)  # revealed: ~A
    reveal_type(i10)  # revealed: Unrelated & ~A
    reveal_type(i11)  # revealed: Unrelated & ~A
    reveal_type(i12)  # revealed: Unrelated & ~A
    reveal_type(i13)  # revealed: Unrelated & ~A
    reveal_type(i14)  # revealed: Unrelated & ~A
    reveal_type(i15)  # revealed: Unrelated & ~A
    reveal_type(i16)  # revealed: Any & ~A
    reveal_type(i17)  # revealed: Any & ~A
    reveal_type(i18)  # revealed: Any & ~A
    reveal_type(i19)  # revealed: Any & ~A
    reveal_type(i20)  # revealed: Any & ~A
    reveal_type(i21)  # revealed: Any & ~A
```

#### Negative type and multiple negative subtypes

If there are multiple negative subtypes, all of them can be removed:

```py
from knot_extensions import Intersection, Not

class A: ...
class B1(A): ...
class B2(A): ...

def _(
    i1: Intersection[Not[A], Not[B1], Not[B2]],
    i2: Intersection[Not[A], Not[B2], Not[B1]],
    i3: Intersection[Not[B1], Not[A], Not[B2]],
    i4: Intersection[Not[B1], Not[B2], Not[A]],
    i5: Intersection[Not[B2], Not[A], Not[B1]],
    i6: Intersection[Not[B2], Not[B1], Not[A]],
) -> None:
    reveal_type(i1)  # revealed: ~A
    reveal_type(i2)  # revealed: ~A
    reveal_type(i3)  # revealed: ~A
    reveal_type(i4)  # revealed: ~A
    reveal_type(i5)  # revealed: ~A
    reveal_type(i6)  # revealed: ~A
```

#### Negative type and positive subtype

When `A` is a supertype of `B`, its negation `~A` is disjoint from `B`, so we can simplify the
intersection to `Never`:

```py
from knot_extensions import Intersection, Not
from typing import Any

class A: ...
class B(A): ...
class C(B): ...
class Unrelated: ...

def _(
    i1: Intersection[Not[A], B],
    i2: Intersection[B, Not[A]],
    i3: Intersection[Not[A], C],
    i4: Intersection[C, Not[A]],
    i5: Intersection[Unrelated, Not[A], B],
    i6: Intersection[B, Not[A], Not[Unrelated]],
    i7: Intersection[Any, Not[A], B],
    i8: Intersection[B, Not[A], Not[Any]],
) -> None:
    reveal_type(i1)  # revealed: Never
    reveal_type(i2)  # revealed: Never
    reveal_type(i3)  # revealed: Never
    reveal_type(i4)  # revealed: Never
    reveal_type(i5)  # revealed: Never
    reveal_type(i6)  # revealed: Never
    reveal_type(i7)  # revealed: Never
    reveal_type(i8)  # revealed: Never
```

### Simplifications of `bool`, `AlwaysTruthy` and `AlwaysFalsy`

In general, intersections with `AlwaysTruthy` and `AlwaysFalsy` cannot be simplified. Naively, you
might think that `int & AlwaysFalsy` could simplify to `Literal[0]`, but this is not the case: for
example, the `False` constant inhabits the type `int & AlwaysFalsy` (due to the fact that
`False.__class__` is `bool` at runtime, and `bool` subclasses `int`), but `False` does not inhabit
the type `Literal[0]`.

Nonetheless, intersections of `AlwaysFalsy` or `AlwaysTruthy` with `bool` _can_ be simplified, due
to the fact that `bool` is a `@final` class at runtime that cannot be subclassed.

```py
from knot_extensions import Intersection, Not, AlwaysTruthy, AlwaysFalsy
from typing_extensions import Literal

class P: ...

def f(
    a: Intersection[bool, AlwaysTruthy],
    b: Intersection[bool, AlwaysFalsy],
    c: Intersection[bool, Not[AlwaysTruthy]],
    d: Intersection[bool, Not[AlwaysFalsy]],
    e: Intersection[bool, AlwaysTruthy, P],
    f: Intersection[bool, AlwaysFalsy, P],
    g: Intersection[bool, Not[AlwaysTruthy], P],
    h: Intersection[bool, Not[AlwaysFalsy], P],
):
    reveal_type(a)  # revealed: Literal[True]
    reveal_type(b)  # revealed: Literal[False]
    reveal_type(c)  # revealed: Literal[False]
    reveal_type(d)  # revealed: Literal[True]

    # `bool & AlwaysTruthy & P` -> `Literal[True] & P` -> `Never`
    reveal_type(e)  # revealed: Never
    reveal_type(f)  # revealed: Never
    reveal_type(g)  # revealed: Never
    reveal_type(h)  # revealed: Never

def never(
    a: Intersection[Intersection[AlwaysFalsy, Not[Literal[False]]], bool],
    b: Intersection[Intersection[AlwaysTruthy, Not[Literal[True]]], bool],
    c: Intersection[Intersection[Literal[True], Not[AlwaysTruthy]], bool],
    d: Intersection[Intersection[Literal[False], Not[AlwaysFalsy]], bool],
):
    # TODO: This should be `Never`
    reveal_type(a)  # revealed: Literal[True]
    # TODO: This should be `Never`
    reveal_type(b)  # revealed: Literal[False]
    reveal_type(c)  # revealed: Never
    reveal_type(d)  # revealed: Never
```

## Simplification of `LiteralString`, `AlwaysTruthy` and `AlwaysFalsy`

Similarly, intersections between `LiteralString`, `AlwaysTruthy` and `AlwaysFalsy` can be
simplified, due to the fact that a `LiteralString` inhabitant is known to have `__class__` set to
exactly `str` (and not a subclass of `str`):

```py
from knot_extensions import Intersection, Not, AlwaysTruthy, AlwaysFalsy, Unknown
from typing_extensions import LiteralString

def f(
    a: Intersection[LiteralString, AlwaysTruthy],
    b: Intersection[LiteralString, AlwaysFalsy],
    c: Intersection[LiteralString, Not[AlwaysTruthy]],
    d: Intersection[LiteralString, Not[AlwaysFalsy]],
    e: Intersection[AlwaysFalsy, LiteralString],
    f: Intersection[Not[AlwaysTruthy], LiteralString],
    g: Intersection[AlwaysTruthy, LiteralString],
    h: Intersection[Not[AlwaysFalsy], LiteralString],
    i: Intersection[Unknown, LiteralString, AlwaysFalsy],
    j: Intersection[Not[AlwaysTruthy], Unknown, LiteralString],
):
    reveal_type(a)  # revealed: LiteralString & ~Literal[""]
    reveal_type(b)  # revealed: Literal[""]
    reveal_type(c)  # revealed: Literal[""]
    reveal_type(d)  # revealed: LiteralString & ~Literal[""]
    reveal_type(e)  # revealed: Literal[""]
    reveal_type(f)  # revealed: Literal[""]
    reveal_type(g)  # revealed: LiteralString & ~Literal[""]
    reveal_type(h)  # revealed: LiteralString & ~Literal[""]
    reveal_type(i)  # revealed: Unknown & Literal[""]
    reveal_type(j)  # revealed: Unknown & Literal[""]
```

## Addition of a type to an intersection with many non-disjoint types

This slightly strange-looking test is a regression test for a mistake that was nearly made in a PR:
<https://github.com/astral-sh/ruff/pull/15475#discussion_r1915041987>.

```py
from knot_extensions import AlwaysFalsy, Intersection, Unknown
from typing_extensions import Literal

def _(x: Intersection[str, Unknown, AlwaysFalsy, Literal[""]]):
    reveal_type(x)  # revealed: Unknown & Literal[""]
```

## Non fully-static types

### Negation of dynamic types

`Any` represents the dynamic type, an unknown set of runtime values. The negation of that, `~Any`,
is still an unknown set of runtime values, so `~Any` is equivalent to `Any`. We therefore eagerly
simplify `~Any` to `Any` in intersections. The same applies to `Unknown`.

```py
from knot_extensions import Intersection, Not, Unknown
from typing_extensions import Any, Never

class P: ...

def any(
    i1: Not[Any],
    i2: Intersection[P, Not[Any]],
    i3: Intersection[Never, Not[Any]],
) -> None:
    reveal_type(i1)  # revealed: Any
    reveal_type(i2)  # revealed: P & Any
    reveal_type(i3)  # revealed: Never

def unknown(
    i1: Not[Unknown],
    i2: Intersection[P, Not[Unknown]],
    i3: Intersection[Never, Not[Unknown]],
) -> None:
    reveal_type(i1)  # revealed: Unknown
    reveal_type(i2)  # revealed: P & Unknown
    reveal_type(i3)  # revealed: Never
```

### Collapsing of multiple `Any`/`Unknown` contributions

The intersection of an unknown set of runtime values with (another) unknown set of runtime values is
still an unknown set of runtime values:

```py
from knot_extensions import Intersection, Not, Unknown
from typing_extensions import Any

class P: ...

def any(
    i1: Intersection[Any, Any],
    i2: Intersection[P, Any, Any],
    i3: Intersection[Any, P, Any],
    i4: Intersection[Any, Any, P],
) -> None:
    reveal_type(i1)  # revealed: Any
    reveal_type(i2)  # revealed: P & Any
    reveal_type(i3)  # revealed: Any & P
    reveal_type(i4)  # revealed: Any & P

def unknown(
    i1: Intersection[Unknown, Unknown],
    i2: Intersection[P, Unknown, Unknown],
    i3: Intersection[Unknown, P, Unknown],
    i4: Intersection[Unknown, Unknown, P],
) -> None:
    reveal_type(i1)  # revealed: Unknown
    reveal_type(i2)  # revealed: P & Unknown
    reveal_type(i3)  # revealed: Unknown & P
    reveal_type(i4)  # revealed: Unknown & P
```

### No self-cancellation

Dynamic types do not cancel each other out. Intersecting an unknown set of values with the negation
of another unknown set of values is not necessarily empty, so we keep the positive contribution:

```py
from typing import Any
from knot_extensions import Intersection, Not, Unknown

def any(
    i1: Intersection[Any, Not[Any]],
    i2: Intersection[Not[Any], Any],
) -> None:
    reveal_type(i1)  # revealed: Any
    reveal_type(i2)  # revealed: Any

def unknown(
    i1: Intersection[Unknown, Not[Unknown]],
    i2: Intersection[Not[Unknown], Unknown],
) -> None:
    reveal_type(i1)  # revealed: Unknown
    reveal_type(i2)  # revealed: Unknown
```

### Mixed dynamic types

We currently do not simplify mixed dynamic types, but might consider doing so in the future:

```py
from typing import Any
from knot_extensions import Intersection, Not, Unknown

def mixed(
    i1: Intersection[Any, Unknown],
    i2: Intersection[Any, Not[Unknown]],
    i3: Intersection[Not[Any], Unknown],
    i4: Intersection[Not[Any], Not[Unknown]],
) -> None:
    reveal_type(i1)  # revealed: Any & Unknown
    reveal_type(i2)  # revealed: Any & Unknown
    reveal_type(i3)  # revealed: Any & Unknown
    reveal_type(i4)  # revealed: Any & Unknown
```

## Invalid

```py
from knot_extensions import Intersection, Not

# error: [invalid-type-form] "`knot_extensions.Intersection` requires at least one argument when used in a type expression"
def f(x: Intersection) -> None:
    reveal_type(x)  # revealed: Unknown

# error: [invalid-type-form] "`knot_extensions.Not` requires exactly one argument when used in a type expression"
def f(x: Not) -> None:
    reveal_type(x)  # revealed: Unknown
```

[complement laws]: https://en.wikipedia.org/wiki/Complement_(set_theory)
[de morgan's laws]: https://en.wikipedia.org/wiki/De_Morgan%27s_laws
