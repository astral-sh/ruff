# Subtype relation

```toml
[environment]
python-version = "3.12"
```

The `is_subtype_of(S, T)` relation below checks if type `S` is a subtype of type `T`.

A fully static type `S` is a subtype of another fully static type `T` iff the set of values
represented by `S` is a subset of the set of values represented by `T`.

A non fully static type `S` can also be safely considered a subtype of a non fully static type `T`,
if all possible materializations of `S` represent sets of values that are a subset of every possible
set of values represented by a materialization of `T`.

See the [typing documentation] for more information.

## Basic builtin types

- `bool` is a subtype of `int`. This is modeled after Python's runtime behavior, where `int` is a
    supertype of `bool` (present in `bool`s bases and MRO).
- `int` is not a subtype of `float`/`complex`, although this is muddied by the
    [special case for float and complex] where annotations of `float` and `complex` are interpreted
    as `int | float` and `int | float | complex`, respectively.

```py
from ty_extensions import is_subtype_of, static_assert, JustFloat, JustComplex

static_assert(is_subtype_of(bool, bool))
static_assert(is_subtype_of(bool, int))
static_assert(is_subtype_of(bool, object))

static_assert(is_subtype_of(int, int))
static_assert(is_subtype_of(int, object))

static_assert(is_subtype_of(object, object))

static_assert(not is_subtype_of(int, bool))
static_assert(not is_subtype_of(int, str))
static_assert(not is_subtype_of(object, int))

static_assert(not is_subtype_of(int, JustFloat))
static_assert(not is_subtype_of(int, JustComplex))

static_assert(is_subtype_of(TypeError, Exception))
static_assert(is_subtype_of(FloatingPointError, Exception))
```

## Class hierarchies

```py
from ty_extensions import is_subtype_of, static_assert
from typing_extensions import Never

class A: ...
class B1(A): ...
class B2(A): ...
class C(B1, B2): ...

static_assert(is_subtype_of(B1, A))
static_assert(not is_subtype_of(A, B1))

static_assert(is_subtype_of(B2, A))
static_assert(not is_subtype_of(A, B2))

static_assert(not is_subtype_of(B1, B2))
static_assert(not is_subtype_of(B2, B1))

static_assert(is_subtype_of(C, B1))
static_assert(is_subtype_of(C, B2))
static_assert(not is_subtype_of(B1, C))
static_assert(not is_subtype_of(B2, C))
static_assert(is_subtype_of(C, A))
static_assert(not is_subtype_of(A, C))

static_assert(is_subtype_of(Never, A))
static_assert(is_subtype_of(Never, B1))
static_assert(is_subtype_of(Never, B2))
static_assert(is_subtype_of(Never, C))

static_assert(is_subtype_of(A, object))
static_assert(is_subtype_of(B1, object))
static_assert(is_subtype_of(B2, object))
static_assert(is_subtype_of(C, object))
```

## Literal types

```py
from typing_extensions import Literal, LiteralString
from ty_extensions import is_subtype_of, static_assert, TypeOf, JustFloat
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

class Single(Enum):
    VALUE = 1

# Boolean literals
static_assert(is_subtype_of(Literal[True], bool))
static_assert(is_subtype_of(Literal[True], int))
static_assert(is_subtype_of(Literal[True], object))

# Integer literals
static_assert(is_subtype_of(Literal[1], int))
static_assert(is_subtype_of(Literal[1], object))

static_assert(not is_subtype_of(Literal[1], bool))

static_assert(not is_subtype_of(Literal[1], JustFloat))

# String literals
static_assert(is_subtype_of(Literal["foo"], LiteralString))
static_assert(is_subtype_of(Literal["foo"], str))
static_assert(is_subtype_of(Literal["foo"], object))

static_assert(is_subtype_of(LiteralString, str))
static_assert(is_subtype_of(LiteralString, object))

# Bytes literals
static_assert(is_subtype_of(Literal[b"foo"], bytes))
static_assert(is_subtype_of(Literal[b"foo"], object))

# Enum literals
static_assert(is_subtype_of(Literal[Answer.YES], Literal[Answer.YES]))
static_assert(is_subtype_of(Literal[Answer.YES], Answer))
static_assert(is_subtype_of(Literal[Answer.YES, Answer.NO], Answer))
static_assert(is_subtype_of(Answer, Literal[Answer.YES, Answer.NO]))

static_assert(not is_subtype_of(Literal[Answer.YES], Literal[Answer.NO]))

static_assert(is_subtype_of(Literal[Single.VALUE], Single))
static_assert(is_subtype_of(Single, Literal[Single.VALUE]))
```

## Heterogeneous tuple types

```py
from ty_extensions import is_subtype_of, static_assert

class A1: ...
class B1(A1): ...
class A2: ...
class B2(A2): ...
class Unrelated: ...

static_assert(is_subtype_of(B1, A1))
static_assert(is_subtype_of(B2, A2))

# Zero-element tuples
static_assert(is_subtype_of(tuple[()], tuple[()]))
static_assert(not is_subtype_of(tuple[()], tuple[Unrelated]))

# One-element tuples
static_assert(is_subtype_of(tuple[B1], tuple[A1]))
static_assert(not is_subtype_of(tuple[B1], tuple[Unrelated]))
static_assert(not is_subtype_of(tuple[B1], tuple[()]))
static_assert(not is_subtype_of(tuple[B1], tuple[A1, Unrelated]))

# Two-element tuples
static_assert(is_subtype_of(tuple[B1, B2], tuple[A1, A2]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[Unrelated, A2]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[A1, Unrelated]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[Unrelated, Unrelated]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[()]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[A1]))
static_assert(not is_subtype_of(tuple[B1, B2], tuple[A1, A2, Unrelated]))
static_assert(is_subtype_of(tuple[int], tuple[object, ...]))
```

## Subtyping of heterogeneous tuple types and homogeneous tuple types

While a homogeneous tuple type is not a subtype of any heterogeneous tuple types, a heterogeneous
tuple type can be a subtype of a homogeneous tuple type, and homogeneous tuple types can be subtypes
of `Sequence`:

```py
from typing import Literal, Any, Sequence
from ty_extensions import static_assert, is_subtype_of, Not, AlwaysFalsy

static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[Literal[1, 2], ...]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[Literal[1], *tuple[Literal[2], ...]]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[*tuple[Literal[1], ...], Literal[2]]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[Literal[1], *tuple[str, ...], Literal[2]]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[Literal[1], Literal[2], *tuple[str, ...]]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[*tuple[str, ...], Literal[1], Literal[2]]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[int, ...]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[int | str, ...]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], tuple[Not[AlwaysFalsy], ...]))
static_assert(is_subtype_of(tuple[Literal[1], Literal[2]], Sequence[int]))
static_assert(is_subtype_of(tuple[int, ...], Sequence[int]))

static_assert(is_subtype_of(tuple[()], tuple[Literal[1, 2], ...]))
static_assert(is_subtype_of(tuple[()], tuple[int, ...]))
static_assert(is_subtype_of(tuple[()], tuple[int | str, ...]))
static_assert(is_subtype_of(tuple[()], tuple[Not[AlwaysFalsy], ...]))
static_assert(is_subtype_of(tuple[()], Sequence[int]))

static_assert(not is_subtype_of(tuple[Literal[1], Literal[2]], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[int, int], tuple[str, ...]))
static_assert(not is_subtype_of(tuple[int, ...], Sequence[Any]))
static_assert(not is_subtype_of(tuple[Any, ...], Sequence[int]))
```

## Subtyping of two mixed tuple types

```py
from typing import Literal, Any, Sequence
from ty_extensions import static_assert, is_subtype_of, Not, AlwaysFalsy

static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[10]],
    )
)
static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...]],
    )
)

static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], *tuple[int, ...], Literal[10]],
    )
)
static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], *tuple[int, ...]],
    )
)

static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[*tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[*tuple[int, ...], Literal[10]],
    )
)
static_assert(
    is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[*tuple[int, ...]],
    )
)

static_assert(
    not is_subtype_of(
        tuple[Literal["foo"], *tuple[int, ...]],
        tuple[int, ...],
    )
)
static_assert(
    not is_subtype_of(
        tuple[*tuple[int, ...], Literal["foo"]],
        tuple[int, ...],
    )
)
static_assert(
    not is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_subtype_of(
        tuple[Literal[1], Literal[2], *tuple[int, ...]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)

static_assert(
    not is_subtype_of(
        tuple[Literal[1], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_subtype_of(
        tuple[Literal[1], *tuple[int, ...], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_subtype_of(
        tuple[Literal[1], *tuple[int, ...]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)

static_assert(
    not is_subtype_of(
        tuple[*tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_subtype_of(
        tuple[*tuple[int, ...], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_subtype_of(
        tuple[*tuple[int, ...]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
```

## Subtyping of the gradual tuple

```toml
[environment]
python-version = "3.12"
```

As a [special case][gradual tuple], `tuple[Any, ...]` is a [gradual][gradual form] tuple type, not
only in the type of its elements, but also in its length.

Its subtyping follows the general rule for subtyping of gradual types.

```py
from typing import Any, Never
from ty_extensions import static_assert, is_subtype_of

static_assert(not is_subtype_of(tuple[Any, ...], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[Any, ...], tuple[Any]))
static_assert(not is_subtype_of(tuple[Any, ...], tuple[Any, Any]))
static_assert(not is_subtype_of(tuple[Any, ...], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[Any, ...], tuple[int]))
static_assert(not is_subtype_of(tuple[Any, ...], tuple[int, int]))
static_assert(is_subtype_of(tuple[Any, ...], tuple[object, ...]))
static_assert(is_subtype_of(tuple[Never, ...], tuple[Any, ...]))
```

Same applies when `tuple[Any, ...]` is unpacked into a mixed tuple.

```py
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[int, *tuple[Any, ...]]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[Any]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[Any, Any]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[int, *tuple[int, ...]]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[int]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...]], tuple[int, int]))

static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[*tuple[Any, ...], int]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[Any]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[Any, Any]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[*tuple[int, ...], int]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[int]))
static_assert(not is_subtype_of(tuple[*tuple[Any, ...], int], tuple[int, int]))

static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[int, *tuple[Any, ...], int]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[Any]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[Any, Any]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[int, *tuple[int, ...], int]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[int]))
static_assert(not is_subtype_of(tuple[int, *tuple[Any, ...], int], tuple[int, int]))
```

Unbounded homogeneous tuples of a non-Any type are defined to be the _union_ of all tuple lengths,
not the _gradual choice_ of them, so no variable-length tuples are a subtype of _any_ fixed-length
tuple.

```py
static_assert(not is_subtype_of(tuple[int, ...], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[int, ...], tuple[Any]))
static_assert(not is_subtype_of(tuple[int, ...], tuple[Any, Any]))
static_assert(is_subtype_of(tuple[int, ...], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[int, ...], tuple[int]))
static_assert(not is_subtype_of(tuple[int, ...], tuple[int, int]))

static_assert(not is_subtype_of(tuple[int, *tuple[int, ...]], tuple[int, *tuple[Any, ...]]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...]], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...]], tuple[Any]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...]], tuple[Any, Any]))
static_assert(is_subtype_of(tuple[int, *tuple[int, ...]], tuple[int, *tuple[int, ...]]))
static_assert(is_subtype_of(tuple[int, *tuple[int, ...]], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...]], tuple[int]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...]], tuple[int, int]))

static_assert(not is_subtype_of(tuple[*tuple[int, ...], int], tuple[*tuple[Any, ...], int]))
static_assert(not is_subtype_of(tuple[*tuple[int, ...], int], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[*tuple[int, ...], int], tuple[Any]))
static_assert(not is_subtype_of(tuple[*tuple[int, ...], int], tuple[Any, Any]))
static_assert(is_subtype_of(tuple[*tuple[int, ...], int], tuple[*tuple[int, ...], int]))
static_assert(is_subtype_of(tuple[*tuple[int, ...], int], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[*tuple[int, ...], int], tuple[int]))
static_assert(not is_subtype_of(tuple[*tuple[int, ...], int], tuple[int, int]))

static_assert(not is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[int, *tuple[Any, ...], int]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[Any, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[Any]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[Any, Any]))
static_assert(is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[int, *tuple[int, ...], int]))
static_assert(is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[int, ...]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[int]))
static_assert(not is_subtype_of(tuple[int, *tuple[int, ...], int], tuple[int, int]))
```

## Union types

```py
from ty_extensions import is_subtype_of, static_assert
from typing import Literal

class A: ...
class B1(A): ...
class B2(A): ...
class Unrelated1: ...
class Unrelated2: ...

static_assert(is_subtype_of(B1, A))
static_assert(is_subtype_of(B2, A))

# Union on the right hand side
static_assert(is_subtype_of(B1, A | Unrelated1))
static_assert(is_subtype_of(B1, Unrelated1 | A))

static_assert(not is_subtype_of(B1, Unrelated1 | Unrelated2))

# Union on the left hand side
static_assert(is_subtype_of(B1 | B2, A))
static_assert(is_subtype_of(B1 | B2 | A, object))

static_assert(not is_subtype_of(B1 | Unrelated1, A))
static_assert(not is_subtype_of(Unrelated1 | B1, A))

# Union on both sides
static_assert(is_subtype_of(B1 | bool, A | int))
static_assert(is_subtype_of(B1 | bool, int | A))

static_assert(not is_subtype_of(B1 | bool, Unrelated1 | int))
static_assert(not is_subtype_of(B1 | bool, int | Unrelated1))

# Example: Unions of literals
static_assert(is_subtype_of(Literal[1, 2, 3], int))
static_assert(not is_subtype_of(Literal[1, "two", 3], int))
```

## Intersection types

```py
from typing_extensions import Literal, LiteralString
from ty_extensions import Intersection, Not, is_subtype_of, static_assert

class A: ...
class B1(A): ...
class B2(A): ...
class C(B1, B2): ...
class Unrelated: ...

static_assert(is_subtype_of(B1, A))
static_assert(is_subtype_of(B2, A))
static_assert(is_subtype_of(C, A))
static_assert(is_subtype_of(C, B1))
static_assert(is_subtype_of(C, B2))

# For complements, the subtyping relation is reversed:
static_assert(is_subtype_of(Not[A], Not[B1]))
static_assert(is_subtype_of(Not[A], Not[B2]))
static_assert(is_subtype_of(Not[A], Not[C]))
static_assert(is_subtype_of(Not[B1], Not[C]))
static_assert(is_subtype_of(Not[B2], Not[C]))

# The intersection of two types is a subtype of both:
static_assert(is_subtype_of(Intersection[B1, B2], B1))
static_assert(is_subtype_of(Intersection[B1, B2], B2))
# … and of their common supertype:
static_assert(is_subtype_of(Intersection[B1, B2], A))

# A common subtype of two types is a subtype of their intersection:
static_assert(is_subtype_of(C, Intersection[B1, B2]))
# … but not the other way around:
static_assert(not is_subtype_of(Intersection[B1, B2], C))

# "Removing" B1 from A leaves a subtype of A.
static_assert(is_subtype_of(Intersection[A, Not[B1]], A))
static_assert(is_subtype_of(Intersection[A, Not[B1]], Not[B1]))

# B1 and B2 are not disjoint, so this is not true:
static_assert(not is_subtype_of(B2, Intersection[A, Not[B1]]))
# … but for two disjoint subtypes, it is:
static_assert(is_subtype_of(Literal[2], Intersection[int, Not[Literal[1]]]))

# A and Unrelated are not related, so this is not true:
static_assert(not is_subtype_of(Intersection[A, Not[B1]], Not[Unrelated]))
# … but for a disjoint type like `None`, it is:
static_assert(is_subtype_of(Intersection[A, Not[B1]], Not[None]))

# Complements of types are still subtypes of `object`:
static_assert(is_subtype_of(Not[A], object))

# More examples:
static_assert(is_subtype_of(type[str], Not[None]))
static_assert(is_subtype_of(Not[LiteralString], object))

static_assert(not is_subtype_of(Intersection[int, Not[Literal[2]]], Intersection[int, Not[Literal[3]]]))
static_assert(not is_subtype_of(Not[Literal[2]], Not[Literal[3]]))
static_assert(not is_subtype_of(Not[Literal[2]], Not[int]))
static_assert(not is_subtype_of(int, Not[Literal[3]]))
static_assert(not is_subtype_of(Literal[1], Intersection[int, Not[Literal[1]]]))
```

## Intersections with non-fully-static negated elements

A type can be a _subtype_ of an intersection containing negated elements only if the _top_
materialization of that type is disjoint from the _top_ materialization of all negated elements in
the intersection. This differs from assignability, which should do the disjointness check against
the _bottom_ materialization of the negated elements.

```py
from typing_extensions import Any, Never, Sequence
from ty_extensions import Not, is_subtype_of, static_assert

# The top materialization of `tuple[Any]` is `tuple[object]`,
# which is disjoint from `tuple[()]` but not `tuple[int]`,
# so `tuple[()]` is a subtype of `~tuple[Any]` but `tuple[int]`
# is not.
static_assert(is_subtype_of(tuple[()], Not[tuple[Any]]))
static_assert(not is_subtype_of(tuple[int], Not[tuple[Any]]))
static_assert(not is_subtype_of(tuple[Any], Not[tuple[Any]]))

# The top materialization of `tuple[Any, ...]` is `tuple[object, ...]`,
# so no tuple type can be considered a subtype of `~tuple[Any, ...]`
static_assert(not is_subtype_of(tuple[()], Not[tuple[Any, ...]]))
static_assert(not is_subtype_of(tuple[int], Not[tuple[Any, ...]]))
static_assert(not is_subtype_of(tuple[int, ...], Not[tuple[Any, ...]]))
static_assert(not is_subtype_of(tuple[object, ...], Not[tuple[Any, ...]]))
static_assert(not is_subtype_of(tuple[Any, ...], Not[tuple[Any, ...]]))

# Similarly, the top materialization of `Sequence[Any]` is `Sequence[object]`,
# so no sequence type can be considered a subtype of `~Sequence[Any]`.
static_assert(not is_subtype_of(tuple[()], Not[Sequence[Any]]))
static_assert(not is_subtype_of(tuple[int], Not[Sequence[Any]]))
static_assert(not is_subtype_of(tuple[int, ...], Not[Sequence[Any]]))
static_assert(not is_subtype_of(tuple[object, ...], Not[Sequence[Any]]))
static_assert(not is_subtype_of(tuple[Any, ...], Not[Sequence[Any]]))
static_assert(not is_subtype_of(list[Never], Not[Sequence[Any]]))
static_assert(not is_subtype_of(list[Any], Not[Sequence[Any]]))
static_assert(not is_subtype_of(list[int], Not[Sequence[Any]]))
```

## Special types

### `Never`

`Never` is a subtype of all types.

```py
from typing_extensions import Literal, Never
from ty_extensions import AlwaysTruthy, AlwaysFalsy, is_subtype_of, static_assert

static_assert(is_subtype_of(Never, Never))
static_assert(is_subtype_of(Never, Literal[True]))
static_assert(is_subtype_of(Never, bool))
static_assert(is_subtype_of(Never, int))
static_assert(is_subtype_of(Never, object))

static_assert(is_subtype_of(Never, AlwaysTruthy))
static_assert(is_subtype_of(Never, AlwaysFalsy))
```

### `AlwaysTruthy` and `AlwaysFalsy`

```toml
[environment]
python-version = "3.11"
```

```py
from ty_extensions import AlwaysTruthy, AlwaysFalsy, Intersection, Not, is_subtype_of, static_assert
from typing_extensions import Literal, LiteralString

static_assert(is_subtype_of(Literal[1], AlwaysTruthy))
static_assert(is_subtype_of(Literal[0], AlwaysFalsy))

static_assert(is_subtype_of(AlwaysTruthy, object))
static_assert(is_subtype_of(AlwaysFalsy, object))

static_assert(not is_subtype_of(Literal[1], AlwaysFalsy))
static_assert(not is_subtype_of(Literal[0], AlwaysTruthy))

static_assert(not is_subtype_of(str, AlwaysTruthy))
static_assert(not is_subtype_of(str, AlwaysFalsy))

# TODO: No errors
# error: [static-assert-error]
static_assert(is_subtype_of(bool, Literal[False] | AlwaysTruthy))
# error: [static-assert-error]
static_assert(is_subtype_of(bool, Literal[True] | AlwaysFalsy))
# error: [static-assert-error]
static_assert(is_subtype_of(LiteralString, Literal[""] | AlwaysTruthy))
static_assert(not is_subtype_of(Literal[True] | AlwaysFalsy, Literal[False] | AlwaysTruthy))

# TODO: No errors
# The condition `is_subtype_of(T & U, U)` must still be satisfied after the following transformations:
# `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal[""]]], AlwaysTruthy))
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal["", "a"]]], AlwaysTruthy))
# `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal[""]]], Not[AlwaysFalsy]))
# error: [static-assert-error]
static_assert(is_subtype_of(Intersection[LiteralString, Not[Literal["", "a"]]], Not[AlwaysFalsy]))

class Length2TupleSubclass(tuple[int, str]): ...

static_assert(is_subtype_of(Length2TupleSubclass, AlwaysTruthy))

class EmptyTupleSubclass(tuple[()]): ...

static_assert(is_subtype_of(EmptyTupleSubclass, AlwaysFalsy))

class TupleSubclassWithAtLeastLength2(tuple[int, *tuple[str, ...], bytes]): ...

static_assert(is_subtype_of(TupleSubclassWithAtLeastLength2, AlwaysTruthy))

class UnknownLength(tuple[int, ...]): ...

static_assert(not is_subtype_of(UnknownLength, AlwaysTruthy))
static_assert(not is_subtype_of(UnknownLength, AlwaysFalsy))

class Invalid(tuple[int, str]):
    # TODO: we should emit an error here (Liskov violation)
    def __bool__(self) -> Literal[False]:
        return False

static_assert(is_subtype_of(Invalid, AlwaysFalsy))
```

### `TypeGuard` and `TypeIs`

Fully-static `TypeGuard[...]` and `TypeIs[...]` are subtypes of `bool`.

```py
from ty_extensions import is_subtype_of, static_assert
from typing_extensions import TypeGuard, TypeIs

# TODO: TypeGuard
# static_assert(is_subtype_of(TypeGuard[int], bool))
# static_assert(is_subtype_of(TypeGuard[int], int))
static_assert(is_subtype_of(TypeIs[str], bool))
static_assert(is_subtype_of(TypeIs[str], int))
```

`TypeIs` is invariant. `TypeGuard` is covariant.

```py
from ty_extensions import is_equivalent_to, is_subtype_of, static_assert
from typing_extensions import TypeGuard, TypeIs

# TODO: TypeGuard
# static_assert(is_subtype_of(TypeGuard[int], TypeGuard[int]))
# static_assert(is_subtype_of(TypeGuard[bool], TypeGuard[int]))
static_assert(is_subtype_of(TypeIs[int], TypeIs[int]))
static_assert(is_subtype_of(TypeIs[int], TypeIs[int]))

static_assert(not is_subtype_of(TypeGuard[int], TypeGuard[bool]))
static_assert(not is_subtype_of(TypeIs[bool], TypeIs[int]))
static_assert(not is_subtype_of(TypeIs[int], TypeIs[bool]))
```

### Module literals

```py
from types import ModuleType
from ty_extensions import TypeOf, is_subtype_of, static_assert
from typing_extensions import assert_type
import typing

assert_type(typing, TypeOf[typing])

static_assert(is_subtype_of(TypeOf[typing], ModuleType))
```

### Slice literals

The type of a slice literal is currently inferred as a specialization of `slice`.

```py
from ty_extensions import TypeOf, is_subtype_of, static_assert

# slice's default specialization is slice[Any, Any, Any], which does not participate in subtyping.
static_assert(not is_subtype_of(TypeOf[1:2:3], slice))
static_assert(is_subtype_of(TypeOf[1:2:3], slice[int]))
```

### Special forms

```py
from typing import _SpecialForm, Literal
from ty_extensions import TypeOf, is_subtype_of, static_assert

static_assert(is_subtype_of(TypeOf[Literal], _SpecialForm))
static_assert(is_subtype_of(TypeOf[Literal], object))

static_assert(not is_subtype_of(_SpecialForm, TypeOf[Literal]))
```

## Class literal types and `type[…]`

### Basic

```py
from typing import _SpecialForm, Any
from typing_extensions import Literal, assert_type
from ty_extensions import TypeOf, is_subtype_of, static_assert

class Meta(type): ...
class HasCustomMetaclass(metaclass=Meta): ...

type LiteralBool = TypeOf[bool]
type LiteralInt = TypeOf[int]
type LiteralStr = TypeOf[str]
type LiteralObject = TypeOf[object]

assert_type(bool, LiteralBool)
assert_type(int, LiteralInt)
assert_type(str, LiteralStr)
assert_type(object, LiteralObject)

# bool

static_assert(is_subtype_of(LiteralBool, LiteralBool))
static_assert(is_subtype_of(LiteralBool, type[bool]))
static_assert(is_subtype_of(LiteralBool, type[int]))
static_assert(is_subtype_of(LiteralBool, type[object]))
static_assert(is_subtype_of(LiteralBool, type))
static_assert(is_subtype_of(LiteralBool, object))

static_assert(not is_subtype_of(LiteralBool, LiteralInt))
static_assert(not is_subtype_of(LiteralBool, LiteralObject))
static_assert(not is_subtype_of(LiteralBool, bool))

static_assert(not is_subtype_of(type, type[bool]))

static_assert(not is_subtype_of(LiteralBool, type[Any]))

# int

static_assert(is_subtype_of(LiteralInt, LiteralInt))
static_assert(is_subtype_of(LiteralInt, type[int]))
static_assert(is_subtype_of(LiteralInt, type[object]))
static_assert(is_subtype_of(LiteralInt, type))
static_assert(is_subtype_of(LiteralInt, object))

static_assert(not is_subtype_of(LiteralInt, LiteralObject))
static_assert(not is_subtype_of(LiteralInt, int))

static_assert(not is_subtype_of(type, type[int]))

static_assert(not is_subtype_of(LiteralInt, type[Any]))

# str

static_assert(is_subtype_of(LiteralStr, type[str]))
static_assert(is_subtype_of(LiteralStr, type))
static_assert(is_subtype_of(LiteralStr, type[object]))

static_assert(not is_subtype_of(type[str], LiteralStr))

static_assert(not is_subtype_of(LiteralStr, type[Any]))

# custom metaclasses

type LiteralHasCustomMetaclass = TypeOf[HasCustomMetaclass]

static_assert(is_subtype_of(LiteralHasCustomMetaclass, Meta))
static_assert(is_subtype_of(Meta, type[object]))
static_assert(is_subtype_of(Meta, type))

static_assert(not is_subtype_of(Meta, type[type]))

static_assert(not is_subtype_of(Meta, type[Any]))

# generics

type LiteralListOfInt = TypeOf[list[int]]

assert_type(list[int], LiteralListOfInt)

static_assert(is_subtype_of(LiteralListOfInt, type))

static_assert(not is_subtype_of(LiteralListOfInt, type[Any]))
```

### Unions of class literals

```py
from typing_extensions import assert_type
from ty_extensions import TypeOf, is_subtype_of, static_assert

class Base: ...
class Derived(Base): ...
class Unrelated: ...

type LiteralBase = TypeOf[Base]
type LiteralDerived = TypeOf[Derived]
type LiteralUnrelated = TypeOf[Unrelated]

assert_type(Base, LiteralBase)
assert_type(Derived, LiteralDerived)
assert_type(Unrelated, LiteralUnrelated)

static_assert(is_subtype_of(LiteralBase, type))
static_assert(is_subtype_of(LiteralBase, object))

static_assert(is_subtype_of(LiteralBase, type[Base]))
static_assert(is_subtype_of(LiteralDerived, type[Base]))
static_assert(is_subtype_of(LiteralDerived, type[Derived]))

static_assert(not is_subtype_of(LiteralBase, type[Derived]))
static_assert(is_subtype_of(type[Derived], type[Base]))

static_assert(is_subtype_of(LiteralBase | LiteralUnrelated, type))
static_assert(is_subtype_of(LiteralBase | LiteralUnrelated, object))
```

## Non-fully-static types

A non-fully-static type can be considered a subtype of another type if all possible materializations
of the first type represent sets of values that are a subset of every possible set of values
represented by a materialization of the second type.

```py
from ty_extensions import Unknown, is_subtype_of, static_assert, Intersection
from typing_extensions import Any

static_assert(not is_subtype_of(Any, Any))
static_assert(not is_subtype_of(Any, int))
static_assert(not is_subtype_of(int, Any))
static_assert(is_subtype_of(Any, object))
static_assert(not is_subtype_of(object, Any))

static_assert(is_subtype_of(int, Any | int))
static_assert(is_subtype_of(Intersection[Any, int], int))
static_assert(not is_subtype_of(tuple[int, int], tuple[int, Any]))

class Covariant[T]:
    def get(self) -> T:
        raise NotImplementedError

static_assert(not is_subtype_of(Covariant[Any], Covariant[Any]))
static_assert(not is_subtype_of(Covariant[Any], Covariant[int]))
static_assert(not is_subtype_of(Covariant[int], Covariant[Any]))
static_assert(is_subtype_of(Covariant[Any], Covariant[object]))
static_assert(not is_subtype_of(Covariant[object], Covariant[Any]))

class Contravariant[T]:
    def receive(self, input: T): ...

static_assert(not is_subtype_of(Contravariant[Any], Contravariant[Any]))
static_assert(not is_subtype_of(Contravariant[Any], Contravariant[int]))
static_assert(not is_subtype_of(Contravariant[int], Contravariant[Any]))
static_assert(not is_subtype_of(Contravariant[Any], Contravariant[object]))
static_assert(is_subtype_of(Contravariant[object], Contravariant[Any]))

class Invariant[T]:
    mutable_attribute: T

static_assert(not is_subtype_of(Invariant[Any], Invariant[Any]))
static_assert(not is_subtype_of(Invariant[Any], Invariant[int]))
static_assert(not is_subtype_of(Invariant[int], Invariant[Any]))
static_assert(not is_subtype_of(Invariant[Any], Invariant[object]))
static_assert(not is_subtype_of(Invariant[object], Invariant[Any]))

class Bivariant[T]: ...

static_assert(is_subtype_of(Bivariant[Any], Bivariant[Any]))
static_assert(is_subtype_of(Bivariant[Any], Bivariant[int]))
static_assert(is_subtype_of(Bivariant[int], Bivariant[Any]))
static_assert(is_subtype_of(Bivariant[Any], Bivariant[object]))
static_assert(is_subtype_of(Bivariant[object], Bivariant[Any]))
```

The same for `Unknown`:

```py
static_assert(not is_subtype_of(Unknown, Unknown))
static_assert(not is_subtype_of(Unknown, int))
static_assert(not is_subtype_of(int, Unknown))
static_assert(is_subtype_of(Unknown, object))
static_assert(not is_subtype_of(object, Unknown))

static_assert(is_subtype_of(int, Unknown | int))
static_assert(is_subtype_of(Intersection[Unknown, int], int))
static_assert(not is_subtype_of(tuple[int, int], tuple[int, Unknown]))
```

Instances of classes that inherit `Any` are not subtypes of some other `Arbitrary` class, because
the `Any` they inherit from could materialize to something (e.g. `object`) that is not a subclass of
that class.

Similarly, they are not subtypes of `Any`, because there are possible materializations of `Any` that
would not satisfy the subtype relation.

They are subtypes of `object`.

```py
class InheritsAny(Any):
    pass

class Arbitrary:
    pass

static_assert(not is_subtype_of(InheritsAny, Arbitrary))
static_assert(not is_subtype_of(InheritsAny, Any))
static_assert(is_subtype_of(InheritsAny, object))
```

Similar for subclass-of types:

```py
static_assert(not is_subtype_of(type[Any], type[Any]))
static_assert(not is_subtype_of(type[object], type[Any]))
static_assert(not is_subtype_of(type[Any], type[Arbitrary]))
static_assert(is_subtype_of(type[Any], type[object]))
```

## Callable

The general principle is that a callable type is a subtype of another if it's more flexible in what
it accepts and more specific in what it returns.

References:

- <https://typing.python.org/en/latest/spec/callables.html#assignability-rules-for-callables>
- <https://typing.python.org/en/latest/spec/callables.html#assignment>

### Return type

Return types are covariant.

```py
from typing import Callable
from ty_extensions import is_subtype_of, static_assert, TypeOf

static_assert(is_subtype_of(Callable[[], int], Callable[[], float]))
static_assert(not is_subtype_of(Callable[[], float], Callable[[], int]))
```

### Optional return type

```py
from typing import Callable
from ty_extensions import is_subtype_of, static_assert, TypeOf

flag: bool = True

def optional_return_type() -> int | None:
    if flag:
        return 1
    return None

def required_return_type() -> int:
    return 1

static_assert(not is_subtype_of(TypeOf[optional_return_type], TypeOf[required_return_type]))
# TypeOf[some_function] is a singleton function-literal type,  not a general callable type
static_assert(not is_subtype_of(TypeOf[required_return_type], TypeOf[optional_return_type]))
static_assert(is_subtype_of(TypeOf[optional_return_type], Callable[[], int | None]))
```

### Parameter types

Parameter types are contravariant.

#### Positional-only

```py
from typing import Callable
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert, TypeOf

def float_param(a: float, /) -> None: ...
def int_param(a: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[float_param], CallableTypeOf[int_param]))
static_assert(not is_subtype_of(CallableTypeOf[int_param], CallableTypeOf[float_param]))

static_assert(is_subtype_of(TypeOf[int_param], Callable[[int], None]))
static_assert(is_subtype_of(TypeOf[float_param], Callable[[float], None]))

static_assert(not is_subtype_of(Callable[[int], None], TypeOf[int_param]))
static_assert(not is_subtype_of(Callable[[float], None], TypeOf[float_param]))
```

Parameter name is not required to be the same for positional-only parameters at the same position:

```py
def int_param_different_name(b: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[int_param], CallableTypeOf[int_param_different_name]))
static_assert(is_subtype_of(CallableTypeOf[int_param_different_name], CallableTypeOf[int_param]))
```

Multiple positional-only parameters are checked in order:

```py
def multi_param1(a: float, b: int, c: str, /) -> None: ...
def multi_param2(b: int, c: bool, a: str, /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[multi_param1], CallableTypeOf[multi_param2]))
static_assert(not is_subtype_of(CallableTypeOf[multi_param2], CallableTypeOf[multi_param1]))

static_assert(is_subtype_of(TypeOf[multi_param1], Callable[[float, int, str], None]))

static_assert(not is_subtype_of(Callable[[float, int, str], None], TypeOf[multi_param1]))
```

#### Positional-only with default value

If the parameter has a default value, it's treated as optional. This means that the parameter at the
corresponding position in the supertype does not need to have a default value.

```py
from typing import Callable
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert, TypeOf

def float_with_default(a: float = 1, /) -> None: ...
def int_with_default(a: int = 1, /) -> None: ...
def int_without_default(a: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[float_with_default], CallableTypeOf[int_with_default]))
static_assert(not is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[float_with_default]))

static_assert(is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[int_without_default]))
static_assert(not is_subtype_of(CallableTypeOf[int_without_default], CallableTypeOf[int_with_default]))

static_assert(is_subtype_of(TypeOf[int_with_default], Callable[[int], None]))
static_assert(is_subtype_of(TypeOf[int_with_default], Callable[[], None]))
static_assert(is_subtype_of(TypeOf[float_with_default], Callable[[float], None]))

static_assert(not is_subtype_of(Callable[[int], None], TypeOf[int_with_default]))
static_assert(not is_subtype_of(Callable[[float], None], TypeOf[float_with_default]))
```

As the parameter itself is optional, it can be omitted in the supertype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[int_without_default], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[int_with_default]))
```

The subtype can include any number of positional-only parameters as long as they have the default
value:

```py
def multi_param(a: float = 1, b: int = 2, c: str = "3", /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[multi_param], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[multi_param]))
```

#### Positional-only with other kinds

If a parameter is declared as positional-only, then the corresponding parameter in the supertype
cannot be any other parameter kind.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def positional_only(a: int, /) -> None: ...
def standard(a: int) -> None: ...
def keyword_only(*, a: int) -> None: ...
def variadic(*a: int) -> None: ...
def keyword_variadic(**a: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[positional_only], CallableTypeOf[standard]))
static_assert(not is_subtype_of(CallableTypeOf[positional_only], CallableTypeOf[keyword_only]))
static_assert(not is_subtype_of(CallableTypeOf[positional_only], CallableTypeOf[variadic]))
static_assert(not is_subtype_of(CallableTypeOf[positional_only], CallableTypeOf[keyword_variadic]))
```

#### Standard

A standard parameter is either a positional or a keyword parameter.

Unlike positional-only parameters, standard parameters should have the same name in the subtype.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def int_param_a(a: int) -> None: ...
def int_param_b(b: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[int_param_a], CallableTypeOf[int_param_b]))
static_assert(not is_subtype_of(CallableTypeOf[int_param_b], CallableTypeOf[int_param_a]))
```

Apart from the name, it behaves the same as positional-only parameters.

```py
def float_param(a: float) -> None: ...
def int_param(a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[float_param], CallableTypeOf[int_param]))
static_assert(not is_subtype_of(CallableTypeOf[int_param], CallableTypeOf[float_param]))
```

With the same rules for default values as well.

```py
def float_with_default(a: float = 1) -> None: ...
def int_with_default(a: int = 1) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[float_with_default], CallableTypeOf[int_with_default]))
static_assert(not is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[float_with_default]))

static_assert(is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[int_param]))
static_assert(not is_subtype_of(CallableTypeOf[int_param], CallableTypeOf[int_with_default]))

static_assert(is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[int_with_default]))
```

Multiple standard parameters are checked in order along with their names:

```py
def multi_param1(a: float, b: int, c: str) -> None: ...
def multi_param2(a: int, b: bool, c: str) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[multi_param1], CallableTypeOf[multi_param2]))
static_assert(not is_subtype_of(CallableTypeOf[multi_param2], CallableTypeOf[multi_param1]))
```

The subtype can include as many standard parameters as long as they have the default value:

```py
def multi_param_default(a: float = 1, b: int = 2, c: str = "s") -> None: ...

static_assert(is_subtype_of(CallableTypeOf[multi_param_default], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[multi_param_default]))
```

#### Standard with keyword-only

A keyword-only parameter in the supertype can be substituted with the corresponding standard
parameter in the subtype with the same name. This is because a standard parameter is more flexible
than a keyword-only parameter.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def standard_a(a: int) -> None: ...
def keyword_b(*, b: int) -> None: ...

# The name of the parameters are different
static_assert(not is_subtype_of(CallableTypeOf[standard_a], CallableTypeOf[keyword_b]))

def standard_float(a: float) -> None: ...
def keyword_int(*, a: int) -> None: ...

# Here, the name of the parameters are the same
static_assert(is_subtype_of(CallableTypeOf[standard_float], CallableTypeOf[keyword_int]))

def standard_with_default(a: int = 1) -> None: ...
def keyword_with_default(*, a: int = 1) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[standard_with_default], CallableTypeOf[keyword_with_default]))
static_assert(is_subtype_of(CallableTypeOf[standard_with_default], CallableTypeOf[empty]))
```

The position of the keyword-only parameters does not matter:

```py
def multi_standard(a: float, b: int, c: str) -> None: ...
def multi_keyword(*, b: bool, c: str, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[multi_standard], CallableTypeOf[multi_keyword]))
```

#### Standard with positional-only

A positional-only parameter in the supertype can be substituted with the corresponding standard
parameter in the subtype at the same position. This is because a standard parameter is more flexible
than a positional-only parameter.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def standard_a(a: int) -> None: ...
def positional_b(b: int, /) -> None: ...

# The names are not important in this context
static_assert(is_subtype_of(CallableTypeOf[standard_a], CallableTypeOf[positional_b]))

def standard_float(a: float) -> None: ...
def positional_int(a: int, /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[standard_float], CallableTypeOf[positional_int]))

def standard_with_default(a: int = 1) -> None: ...
def positional_with_default(a: int = 1, /) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[standard_with_default], CallableTypeOf[positional_with_default]))
static_assert(is_subtype_of(CallableTypeOf[standard_with_default], CallableTypeOf[empty]))
```

The position of the positional-only parameters matter:

```py
def multi_standard(a: float, b: int, c: str) -> None: ...
def multi_positional1(b: int, c: bool, a: str, /) -> None: ...

# Here, the type of the parameter `a` makes the subtype relation invalid
def multi_positional2(b: int, a: float, c: str, /) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[multi_standard], CallableTypeOf[multi_positional1]))
static_assert(not is_subtype_of(CallableTypeOf[multi_standard], CallableTypeOf[multi_positional2]))
```

#### Standard with variadic

A variadic or keyword-variadic parameter in the supertype cannot be substituted with a standard
parameter in the subtype.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def standard(a: int) -> None: ...
def variadic(*a: int) -> None: ...
def keyword_variadic(**a: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[standard], CallableTypeOf[variadic]))
static_assert(not is_subtype_of(CallableTypeOf[standard], CallableTypeOf[keyword_variadic]))
```

#### Variadic

The name of the variadic parameter does not need to be the same in the subtype.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def variadic_float(*args2: float) -> None: ...
def variadic_int(*args1: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[variadic_float], CallableTypeOf[variadic_int]))
static_assert(not is_subtype_of(CallableTypeOf[variadic_int], CallableTypeOf[variadic_float]))
```

The variadic parameter does not need to be present in the supertype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[variadic_int], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[variadic_int]))
```

#### Variadic with positional-only

If the subtype has a variadic parameter then any unmatched positional-only parameter from the
supertype should be checked against the variadic parameter.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def variadic(a: int, /, *args: float) -> None: ...

# Here, the parameter `b` and `c` are unmatched
def positional_only(a: int, b: float, c: int, /) -> None: ...

# Here, the parameter `b` is unmatched and there's also a variadic parameter
def positional_variadic(a: int, b: float, /, *args: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[variadic], CallableTypeOf[positional_only]))
static_assert(is_subtype_of(CallableTypeOf[variadic], CallableTypeOf[positional_variadic]))
```

#### Variadic with other kinds

Variadic parameter in a subtype can only be used to match against an unmatched positional-only
parameters from the supertype, not any other parameter kind.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def variadic(*args: int) -> None: ...

# Both positional-only parameters are unmatched so uses the variadic parameter but the other
# parameter `c` remains and cannot be matched.
def standard(a: int, b: float, /, c: int) -> None: ...

# Similarly, for other kinds
def keyword_only(a: int, /, *, b: int) -> None: ...
def keyword_variadic(a: int, /, **kwargs: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[variadic], CallableTypeOf[standard]))
static_assert(not is_subtype_of(CallableTypeOf[variadic], CallableTypeOf[keyword_only]))
static_assert(not is_subtype_of(CallableTypeOf[variadic], CallableTypeOf[keyword_variadic]))
```

But, there are special cases when matching against standard parameters. This is due to the fact that
a standard parameter can be passed as a positional or keyword parameter. This means that the
subtyping relation needs to consider both cases.

```py
def variadic_keyword(*args: int, **kwargs: int) -> None: ...
def standard_int(a: int) -> None: ...
def standard_float(a: float) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[variadic_keyword], CallableTypeOf[standard_int]))
static_assert(not is_subtype_of(CallableTypeOf[variadic_keyword], CallableTypeOf[standard_float]))
```

If the type of either the variadic or keyword-variadic parameter is not a supertype of the standard
parameter, then the subtyping relation is invalid.

```py
def variadic_bool(*args: bool, **kwargs: int) -> None: ...
def keyword_variadic_bool(*args: int, **kwargs: bool) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[variadic_bool], CallableTypeOf[standard_int]))
static_assert(not is_subtype_of(CallableTypeOf[keyword_variadic_bool], CallableTypeOf[standard_int]))
```

The standard parameter can follow a variadic parameter in the subtype.

```py
def standard_variadic_int(a: int, *args: int) -> None: ...
def standard_variadic_float(a: int, *args: float) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[variadic_keyword], CallableTypeOf[standard_variadic_int]))
static_assert(not is_subtype_of(CallableTypeOf[variadic_keyword], CallableTypeOf[standard_variadic_float]))
```

The keyword part of the standard parameter can be matched against keyword-only parameter with the
same name if the keyword-variadic parameter is absent.

```py
def variadic_a(*args: int, a: int) -> None: ...
def variadic_b(*args: int, b: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[variadic_a], CallableTypeOf[standard_int]))
# The parameter name is different
static_assert(not is_subtype_of(CallableTypeOf[variadic_b], CallableTypeOf[standard_int]))
```

#### Keyword-only

For keyword-only parameters, the name should be the same:

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def keyword_int(*, a: int) -> None: ...
def keyword_float(*, a: float) -> None: ...
def keyword_b(*, b: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[keyword_float], CallableTypeOf[keyword_int]))
static_assert(not is_subtype_of(CallableTypeOf[keyword_int], CallableTypeOf[keyword_float]))
static_assert(not is_subtype_of(CallableTypeOf[keyword_int], CallableTypeOf[keyword_b]))
```

But, the order of the keyword-only parameters is not required to be the same:

```py
def keyword_ab(*, a: float, b: float) -> None: ...
def keyword_ba(*, b: int, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[keyword_ab], CallableTypeOf[keyword_ba]))
static_assert(not is_subtype_of(CallableTypeOf[keyword_ba], CallableTypeOf[keyword_ab]))
```

#### Keyword-only with default

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def float_with_default(*, a: float = 1) -> None: ...
def int_with_default(*, a: int = 1) -> None: ...
def int_keyword(*, a: int) -> None: ...
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[float_with_default], CallableTypeOf[int_with_default]))
static_assert(not is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[float_with_default]))

static_assert(is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[int_keyword]))
static_assert(not is_subtype_of(CallableTypeOf[int_keyword], CallableTypeOf[int_with_default]))

static_assert(is_subtype_of(CallableTypeOf[int_with_default], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[int_with_default]))
```

Keyword-only parameters with default values can be mixed with the ones without default values in any
order:

```py
# A keyword-only parameter with a default value follows the one without a default value (it's valid)
def mixed(*, b: int = 1, a: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[mixed], CallableTypeOf[int_keyword]))
static_assert(not is_subtype_of(CallableTypeOf[int_keyword], CallableTypeOf[mixed]))
```

#### Keyword-only with standard

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def keywords1(*, a: int, b: int) -> None: ...
def standard(b: float, a: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[keywords1], CallableTypeOf[standard]))
static_assert(is_subtype_of(CallableTypeOf[standard], CallableTypeOf[keywords1]))
```

The subtype can include additional standard parameters as long as it has the default value:

```py
def standard_with_default(b: float, a: float, c: float = 1) -> None: ...
def standard_without_default(b: float, a: float, c: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[standard_without_default], CallableTypeOf[keywords1]))
static_assert(is_subtype_of(CallableTypeOf[standard_with_default], CallableTypeOf[keywords1]))
```

Here, we mix keyword-only parameters with standard parameters:

```py
def keywords2(*, a: int, c: int, b: int) -> None: ...
def mixed(b: float, a: float, *, c: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[keywords2], CallableTypeOf[mixed]))
static_assert(is_subtype_of(CallableTypeOf[mixed], CallableTypeOf[keywords2]))
```

But, we shouldn't consider any unmatched positional-only parameters:

```py
def mixed_positional(b: float, /, a: float, *, c: float) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[mixed_positional], CallableTypeOf[keywords2]))
```

But, an unmatched variadic parameter is still valid:

```py
def mixed_variadic(*args: float, a: float, b: float, c: float, **kwargs: float) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[mixed_variadic], CallableTypeOf[keywords2]))
```

#### Keyword-variadic

The name of the keyword-variadic parameter does not need to be the same in the subtype.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def kwargs_float(**kwargs2: float) -> None: ...
def kwargs_int(**kwargs1: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[kwargs_float], CallableTypeOf[kwargs_int]))
static_assert(not is_subtype_of(CallableTypeOf[kwargs_int], CallableTypeOf[kwargs_float]))
```

A variadic parameter can be omitted in the subtype:

```py
def empty() -> None: ...

static_assert(is_subtype_of(CallableTypeOf[kwargs_int], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[kwargs_int]))
```

#### Keyword-variadic with keyword-only

If the subtype has a keyword-variadic parameter then any unmatched keyword-only parameter from the
supertype should be checked against the keyword-variadic parameter.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def kwargs(**kwargs: float) -> None: ...
def keyword_only(*, a: int, b: float, c: bool) -> None: ...
def keyword_variadic(*, a: int, **kwargs: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[kwargs], CallableTypeOf[keyword_only]))
static_assert(is_subtype_of(CallableTypeOf[kwargs], CallableTypeOf[keyword_variadic]))
```

This is valid only for keyword-only parameters, not any other parameter kind:

```py
def mixed1(a: int, *, b: int) -> None: ...

# Same as above but with the default value
def mixed2(a: int = 1, *, b: int) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[kwargs], CallableTypeOf[mixed1]))
static_assert(not is_subtype_of(CallableTypeOf[kwargs], CallableTypeOf[mixed2]))
```

#### Empty

When the supertype has an empty list of parameters, then the subtype can have any kind of parameters
as long as they contain the default values for non-variadic parameters.

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def empty() -> None: ...
def mixed(a: int = 1, /, b: int = 2, *args: int, c: int = 3, **kwargs: int) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[mixed], CallableTypeOf[empty]))
static_assert(not is_subtype_of(CallableTypeOf[empty], CallableTypeOf[mixed]))
```

#### Object

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert, TypeOf
from typing import Callable

def f1(a: int, b: str, /, *c: float, d: int = 1, **e: float) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[f1], object))
static_assert(not is_subtype_of(object, CallableTypeOf[f1]))

def _(
    f3: Callable[[int, str], None],
) -> None:
    static_assert(is_subtype_of(TypeOf[f3], object))
    static_assert(not is_subtype_of(object, TypeOf[f3]))

class C:
    def foo(self) -> None: ...

static_assert(is_subtype_of(TypeOf[C.foo], object))
static_assert(not is_subtype_of(object, TypeOf[C.foo]))
```

#### Gradual form

A callable type with `...` parameters can be considered a supertype of a callable type that accepts
any arguments of any type, but otherwise is not a subtype or supertype of any callable type.

```py
from typing import Callable, Never
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

def bottom(*args: object, **kwargs: object) -> Never:
    raise Exception()

type BottomCallable = CallableTypeOf[bottom]

static_assert(is_subtype_of(BottomCallable, Callable[..., Never]))
static_assert(is_subtype_of(BottomCallable, Callable[..., int]))

static_assert(not is_subtype_of(Callable[[], object], Callable[..., object]))
static_assert(not is_subtype_of(Callable[..., object], Callable[[], object]))
```

According to the spec, `*args: Any, **kwargs: Any` is equivalent to `...`. This is a subtle but
important distinction. No materialization of the former signature (if taken literally) can have any
required arguments, but `...` can materialize to a signature with required arguments. The below test
would not pass if we didn't handle this special case.

```py
from typing import Callable, Any
from ty_extensions import is_subtype_of, static_assert, CallableTypeOf

def f(*args: Any, **kwargs: Any) -> Any: ...

static_assert(not is_subtype_of(CallableTypeOf[f], Callable[[], object]))
```

### Classes with `__call__`

```py
from typing import Callable, Any
from ty_extensions import TypeOf, is_subtype_of, static_assert, is_assignable_to

class A:
    def __call__(self, a: int) -> int:
        return a

a = A()

static_assert(is_subtype_of(A, Callable[[int], int]))
static_assert(not is_subtype_of(A, Callable[[], int]))
static_assert(not is_subtype_of(Callable[[int], int], A))
static_assert(not is_subtype_of(A, Callable[[Any], int]))
static_assert(not is_subtype_of(A, Callable[[int], Any]))

def f(fn: Callable[[int], int]) -> None: ...

f(a)
```

### Classes with `__call__` as attribute

An instance type can be a subtype of a compatible callable type if the instance type's class has a
callable `__call__` attribute.

```py
from __future__ import annotations

from typing import Callable
from ty_extensions import static_assert, is_subtype_of

def call_impl(a: A, x: int) -> str:
    return ""

class A:
    __call__: Callable[[A, int], str] = call_impl

static_assert(is_subtype_of(A, Callable[[int], str]))
static_assert(not is_subtype_of(A, Callable[[int], int]))
reveal_type(A()(1))  # revealed: str
```

### Class literals

This section also tests assignability of class-literals to callback protocols, since the rules for
assignability of class-literals to callback protocols are the same as the rules for assignability of
class-literals to `Callable` types.

```toml
[environment]
python-version = "3.12"
```

#### Classes with metaclasses

```py
from typing import Callable, Protocol, overload
from typing_extensions import Self
from ty_extensions import TypeOf, static_assert, is_subtype_of

class MetaWithReturn(type):
    def __call__(cls) -> "A":
        return super().__call__()

class A(metaclass=MetaWithReturn): ...

class Returns[T](Protocol):
    def __call__(self) -> T: ...

class ReturnsWithArgument[T1, T2](Protocol):
    def __call__(self, arg: T1, /) -> T2: ...

static_assert(is_subtype_of(TypeOf[A], Callable[[], A]))
static_assert(is_subtype_of(TypeOf[A], Returns[A]))
static_assert(not is_subtype_of(TypeOf[A], Callable[[object], A]))
static_assert(not is_subtype_of(TypeOf[A], ReturnsWithArgument[object, A]))

class MetaWithDifferentReturn(type):
    def __call__(cls) -> int:
        return super().__call__()

class B(metaclass=MetaWithDifferentReturn): ...

static_assert(is_subtype_of(TypeOf[B], Callable[[], int]))
static_assert(is_subtype_of(TypeOf[B], Returns[int]))
static_assert(not is_subtype_of(TypeOf[B], Callable[[], B]))
static_assert(not is_subtype_of(TypeOf[B], Returns[B]))

class MetaWithOverloadReturn(type):
    @overload
    def __call__(cls, x: int) -> int: ...
    @overload
    def __call__(cls) -> str: ...
    def __call__(cls, x: int | None = None) -> str | int:
        return super().__call__()

class C(metaclass=MetaWithOverloadReturn): ...

static_assert(is_subtype_of(TypeOf[C], Callable[[int], int]))
static_assert(is_subtype_of(TypeOf[C], Callable[[], str]))
static_assert(is_subtype_of(TypeOf[C], ReturnsWithArgument[int, int]))
static_assert(is_subtype_of(TypeOf[C], Returns[str]))
```

#### Classes with `__new__`

```py
from typing import Callable, overload, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of

class A:
    def __new__(cls, a: int) -> int:
        return a

class Returns[T](Protocol):
    def __call__(self) -> T: ...

class ReturnsWithArgument[T1, T2](Protocol):
    def __call__(self, arg: T1, /) -> T2: ...

static_assert(is_subtype_of(TypeOf[A], Callable[[int], int]))
static_assert(is_subtype_of(TypeOf[A], ReturnsWithArgument[int, int]))
static_assert(not is_subtype_of(TypeOf[A], Callable[[], int]))
static_assert(not is_subtype_of(TypeOf[A], Returns[int]))

class B: ...
class C(B): ...

class D:
    def __new__(cls) -> B:
        return B()

class E(D):
    def __new__(cls) -> C:
        return C()

static_assert(is_subtype_of(TypeOf[E], Callable[[], C]))
static_assert(is_subtype_of(TypeOf[E], Returns[C]))
static_assert(is_subtype_of(TypeOf[E], Callable[[], B]))
static_assert(is_subtype_of(TypeOf[E], Returns[B]))
static_assert(not is_subtype_of(TypeOf[D], Callable[[], C]))
static_assert(not is_subtype_of(TypeOf[D], Returns[C]))
static_assert(is_subtype_of(TypeOf[D], Callable[[], B]))
static_assert(is_subtype_of(TypeOf[D], Returns[B]))

class F:
    @overload
    def __new__(cls) -> int: ...
    @overload
    def __new__(cls, x: int) -> "F": ...
    def __new__(cls, x: int | None = None) -> "int | F":
        return 1 if x is None else object.__new__(cls)

    def __init__(self, y: str) -> None: ...

static_assert(is_subtype_of(TypeOf[F], Callable[[int], F]))
static_assert(is_subtype_of(TypeOf[F], Callable[[], int]))
static_assert(not is_subtype_of(TypeOf[F], Callable[[str], F]))
```

#### Classes with `__call__` and `__new__`

If `__call__` and `__new__` are both present, `__call__` takes precedence.

```py
from typing import Callable, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of

class MetaWithIntReturn(type):
    def __call__(cls) -> int:
        return super().__call__()

class F(metaclass=MetaWithIntReturn):
    def __new__(cls) -> str:
        return super().__new__(cls)

class Returns[T](Protocol):
    def __call__(self) -> T: ...

static_assert(is_subtype_of(TypeOf[F], Callable[[], int]))
static_assert(is_subtype_of(TypeOf[F], Returns[int]))
static_assert(not is_subtype_of(TypeOf[F], Callable[[], str]))
static_assert(not is_subtype_of(TypeOf[F], Returns[str]))
```

#### Classes with `__init__`

```py
from typing import Callable, overload, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of

class Returns[T](Protocol):
    def __call__(self) -> T: ...

class ReturnsWithArgument[T1, T2](Protocol):
    def __call__(self, arg: T1, /) -> T2: ...

class A:
    def __init__(self, a: int) -> None: ...

static_assert(is_subtype_of(TypeOf[A], Callable[[int], A]))
static_assert(is_subtype_of(TypeOf[A], ReturnsWithArgument[int, A]))
static_assert(not is_subtype_of(TypeOf[A], Callable[[], A]))
static_assert(not is_subtype_of(TypeOf[A], Returns[A]))

class B:
    @overload
    def __init__(self, a: int) -> None: ...
    @overload
    def __init__(self) -> None: ...
    def __init__(self, a: int | None = None) -> None: ...

static_assert(is_subtype_of(TypeOf[B], Callable[[int], B]))
static_assert(is_subtype_of(TypeOf[B], ReturnsWithArgument[int, B]))
static_assert(is_subtype_of(TypeOf[B], Callable[[], B]))
static_assert(is_subtype_of(TypeOf[B], Returns[B]))

class D[T]:
    def __init__(self, x: T) -> None: ...

static_assert(is_subtype_of(TypeOf[D[int]], Callable[[int], D[int]]))
static_assert(is_subtype_of(TypeOf[D[int]], ReturnsWithArgument[int, D[int]]))
static_assert(not is_subtype_of(TypeOf[D[int]], Callable[[str], D[int]]))
static_assert(not is_subtype_of(TypeOf[D[int]], ReturnsWithArgument[str, D[int]]))
```

#### Classes with `__init__` and `__new__`

```py
from typing import Callable, overload, Self, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of

class Returns[T](Protocol):
    def __call__(self) -> T: ...

class ReturnsWithArgument[T1, T2](Protocol):
    def __call__(self, arg: T1, /) -> T2: ...

class A:
    def __new__(cls, a: int) -> Self:
        return super().__new__(cls)

    def __init__(self, a: int) -> None: ...

static_assert(is_subtype_of(TypeOf[A], Callable[[int], A]))
static_assert(is_subtype_of(TypeOf[A], ReturnsWithArgument[int, A]))
static_assert(not is_subtype_of(TypeOf[A], Callable[[], A]))
static_assert(not is_subtype_of(TypeOf[A], Returns[A]))

class B:
    def __new__(cls, a: int) -> int:
        return super().__new__(cls)

    def __init__(self, a: str) -> None: ...

static_assert(is_subtype_of(TypeOf[B], Callable[[int], int]))
static_assert(is_subtype_of(TypeOf[B], ReturnsWithArgument[int, int]))
static_assert(not is_subtype_of(TypeOf[B], Callable[[str], B]))
static_assert(not is_subtype_of(TypeOf[B], ReturnsWithArgument[str, B]))

class C:
    def __new__(cls, *args, **kwargs) -> "C":
        return super().__new__(cls)

    def __init__(self, x: int) -> None: ...

# Not subtype because __new__ signature is not fully static
static_assert(not is_subtype_of(TypeOf[C], Callable[[int], C]))
static_assert(not is_subtype_of(TypeOf[C], ReturnsWithArgument[int, C]))
static_assert(not is_subtype_of(TypeOf[C], Callable[[], C]))
static_assert(not is_subtype_of(TypeOf[C], Returns[C]))

class D: ...

class E:
    @overload
    def __new__(cls) -> int: ...
    @overload
    def __new__(cls, x: int) -> D: ...
    def __new__(cls, x: int | None = None) -> int | D:
        return D()

    def __init__(self, y: str) -> None: ...

static_assert(is_subtype_of(TypeOf[E], Callable[[int], D]))
static_assert(is_subtype_of(TypeOf[E], ReturnsWithArgument[int, D]))
static_assert(is_subtype_of(TypeOf[E], Callable[[], int]))
static_assert(is_subtype_of(TypeOf[E], Returns[int]))

class F[T]:
    def __new__(cls, x: T) -> "F[T]":
        return super().__new__(cls)

    def __init__(self, x: T) -> None: ...

static_assert(is_subtype_of(TypeOf[F[int]], Callable[[int], F[int]]))
static_assert(is_subtype_of(TypeOf[F[int]], ReturnsWithArgument[int, F[int]]))
static_assert(not is_subtype_of(TypeOf[F[int]], Callable[[str], F[int]]))
static_assert(not is_subtype_of(TypeOf[F[int]], ReturnsWithArgument[str, F[int]]))
```

#### Classes with `__call__`, `__new__` and `__init__`

If `__call__`, `__new__` and `__init__` are all present, `__call__` takes precedence.

```py
from typing import Callable, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of

class Returns[T](Protocol):
    def __call__(self) -> T: ...

class ReturnsWithArgument[T1, T2](Protocol):
    def __call__(self, arg: T1, /) -> T2: ...

class MetaWithIntReturn(type):
    def __call__(cls) -> int:
        return super().__call__()

class F(metaclass=MetaWithIntReturn):
    def __new__(cls) -> str:
        return super().__new__(cls)

    def __init__(self, x: int) -> None: ...

static_assert(is_subtype_of(TypeOf[F], Callable[[], int]))
static_assert(is_subtype_of(TypeOf[F], Returns[int]))
static_assert(not is_subtype_of(TypeOf[F], Callable[[], str]))
static_assert(not is_subtype_of(TypeOf[F], Returns[str]))
static_assert(not is_subtype_of(TypeOf[F], Callable[[int], F]))
static_assert(not is_subtype_of(TypeOf[F], ReturnsWithArgument[int, F]))
```

### Classes with no constructor methods

```py
from typing import Callable, Protocol
from ty_extensions import TypeOf, static_assert, is_subtype_of

class Returns[T](Protocol):
    def __call__(self) -> T: ...

class A: ...

static_assert(is_subtype_of(TypeOf[A], Callable[[], A]))
static_assert(is_subtype_of(TypeOf[A], Returns[A]))
```

### Subclass of

#### Type of a class with constructor methods

```py
from typing import Callable
from ty_extensions import TypeOf, static_assert, is_subtype_of

class A:
    def __init__(self, x: int) -> None: ...

class B:
    def __new__(cls, x: str) -> "B":
        return super().__new__(cls)

static_assert(is_subtype_of(type[A], Callable[[int], A]))
static_assert(not is_subtype_of(type[A], Callable[[str], A]))

static_assert(is_subtype_of(type[B], Callable[[str], B]))
static_assert(not is_subtype_of(type[B], Callable[[int], B]))
```

### Dataclasses

Dataclasses synthesize a `__init__` method.

```py
from typing import Callable
from ty_extensions import TypeOf, static_assert, is_subtype_of
from dataclasses import dataclass

@dataclass
class A:
    x: "A" | None

static_assert(is_subtype_of(type[A], Callable[[A], A]))
static_assert(is_subtype_of(type[A], Callable[[None], A]))
static_assert(is_subtype_of(type[A], Callable[[A | None], A]))
static_assert(not is_subtype_of(type[A], Callable[[int], A]))
```

### Bound methods

```py
from typing import Callable
from ty_extensions import TypeOf, static_assert, is_subtype_of

class A:
    def f(self, a: int) -> int:
        return a

    @classmethod
    def g(cls, a: int) -> int:
        return a

a = A()

static_assert(is_subtype_of(TypeOf[a.f], Callable[[int], int]))
static_assert(is_subtype_of(TypeOf[a.g], Callable[[int], int]))
static_assert(is_subtype_of(TypeOf[A.g], Callable[[int], int]))

static_assert(not is_subtype_of(TypeOf[a.f], Callable[[float], int]))
static_assert(not is_subtype_of(TypeOf[A.g], Callable[[], int]))

static_assert(is_subtype_of(TypeOf[A.f], Callable[[A, int], int]))
```

### Overloads

#### Subtype overloaded

For `B <: A`, if a callable `B` is overloaded with two or more signatures, it is a subtype of
callable `A` if _at least one_ of the overloaded signatures in `B` is a subtype of `A`.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...
class C: ...

@overload
def overloaded(x: A) -> None: ...
@overload
def overloaded(x: B) -> None: ...
```

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert
from overloaded import A, B, C, overloaded

def accepts_a(x: A) -> None: ...
def accepts_b(x: B) -> None: ...
def accepts_c(x: C) -> None: ...

static_assert(is_subtype_of(CallableTypeOf[overloaded], CallableTypeOf[accepts_a]))
static_assert(is_subtype_of(CallableTypeOf[overloaded], CallableTypeOf[accepts_b]))
static_assert(not is_subtype_of(CallableTypeOf[overloaded], CallableTypeOf[accepts_c]))
```

#### Supertype overloaded

For `B <: A`, if a callable `A` is overloaded with two or more signatures, callable `B` is a subtype
of `A` if `B` is a subtype of _all_ of the signatures in `A`.

`overloaded.pyi`:

```pyi
from typing import overload

class Grandparent: ...
class Parent(Grandparent): ...
class Child(Parent): ...

@overload
def overloaded(a: Child) -> None: ...
@overload
def overloaded(a: Parent) -> None: ...
@overload
def overloaded(a: Grandparent) -> None: ...
```

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert
from overloaded import Grandparent, Parent, Child, overloaded

# This is a subtype of only the first overload
def child(a: Child) -> None: ...

# This is a subtype of the first and second overload
def parent(a: Parent) -> None: ...

# This is the only function that's a subtype of all overloads
def grandparent(a: Grandparent) -> None: ...

static_assert(not is_subtype_of(CallableTypeOf[child], CallableTypeOf[overloaded]))
static_assert(not is_subtype_of(CallableTypeOf[parent], CallableTypeOf[overloaded]))
static_assert(is_subtype_of(CallableTypeOf[grandparent], CallableTypeOf[overloaded]))
```

#### Both overloads

For `B <: A`, if both `A` and `B` is a callable that's overloaded with two or more signatures, then
`B` is a subtype of `A` if for _every_ signature in `A`, there is _at least one_ signature in `B`
that is a subtype of it.

`overloaded.pyi`:

```pyi
from typing import overload

class Grandparent: ...
class Parent(Grandparent): ...
class Child(Parent): ...
class Other: ...

@overload
def pg(a: Parent) -> None: ...
@overload
def pg(a: Grandparent) -> None: ...

@overload
def po(a: Parent) -> None: ...
@overload
def po(a: Other) -> None: ...

@overload
def go(a: Grandparent) -> None: ...
@overload
def go(a: Other) -> None: ...

@overload
def cpg(a: Child) -> None: ...
@overload
def cpg(a: Parent) -> None: ...
@overload
def cpg(a: Grandparent) -> None: ...

@overload
def empty_go() -> Child: ...
@overload
def empty_go(a: Grandparent) -> None: ...
@overload
def empty_go(a: Other) -> Other: ...

@overload
def empty_cp() -> Parent: ...
@overload
def empty_cp(a: Child) -> None: ...
@overload
def empty_cp(a: Parent) -> None: ...
```

```py
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert
from overloaded import pg, po, go, cpg, empty_go, empty_cp

static_assert(is_subtype_of(CallableTypeOf[pg], CallableTypeOf[cpg]))
static_assert(is_subtype_of(CallableTypeOf[cpg], CallableTypeOf[pg]))

static_assert(not is_subtype_of(CallableTypeOf[po], CallableTypeOf[pg]))
static_assert(not is_subtype_of(CallableTypeOf[pg], CallableTypeOf[po]))

static_assert(is_subtype_of(CallableTypeOf[go], CallableTypeOf[pg]))
static_assert(not is_subtype_of(CallableTypeOf[pg], CallableTypeOf[go]))

# Overload 1 in `empty_go` is a subtype of overload 1 in `empty_cp`
# Overload 2 in `empty_go` is a subtype of overload 2 in `empty_cp`
# Overload 2 in `empty_go` is a subtype of overload 3 in `empty_cp`
#
# All overloads in `empty_cp` has a subtype in `empty_go`
static_assert(is_subtype_of(CallableTypeOf[empty_go], CallableTypeOf[empty_cp]))

static_assert(not is_subtype_of(CallableTypeOf[empty_cp], CallableTypeOf[empty_go]))
```

#### Order of overloads

Order of overloads is irrelevant for subtyping.

`overloaded.pyi`:

```pyi
from typing import overload

class A: ...
class B: ...

@overload
def overload_ab(x: A) -> None: ...
@overload
def overload_ab(x: B) -> None: ...

@overload
def overload_ba(x: B) -> None: ...
@overload
def overload_ba(x: A) -> None: ...
```

```py
from overloaded import overload_ab, overload_ba
from ty_extensions import CallableTypeOf, is_subtype_of, static_assert

static_assert(is_subtype_of(CallableTypeOf[overload_ab], CallableTypeOf[overload_ba]))
static_assert(is_subtype_of(CallableTypeOf[overload_ba], CallableTypeOf[overload_ab]))
```

### Generic callables

A generic callable can be considered equivalent to an intersection of all of its possible
specializations. That means that a generic callable is a subtype of any particular specialization.
(If someone expects a function that works with a particular specialization, it's fine to hand them
the generic callable.)

```py
from typing import Callable
from ty_extensions import CallableTypeOf, TypeOf, is_subtype_of, static_assert

def identity[T](t: T) -> T:
    return t

# TODO: Confusingly, these are not the same results as the corresponding checks in
# is_assignable_to.md, even though all of these types are fully static. We have some heuristics that
# currently conflict with each other, that we are in the process of removing with the constraint set
# work.
# TODO: no error
# error: [static-assert-error]
static_assert(is_subtype_of(TypeOf[identity], Callable[[int], int]))
# TODO: no error
# error: [static-assert-error]
static_assert(is_subtype_of(TypeOf[identity], Callable[[str], str]))
static_assert(not is_subtype_of(TypeOf[identity], Callable[[str], int]))

# TODO: no error
# error: [static-assert-error]
static_assert(is_subtype_of(CallableTypeOf[identity], Callable[[int], int]))
# TODO: no error
# error: [static-assert-error]
static_assert(is_subtype_of(CallableTypeOf[identity], Callable[[str], str]))
static_assert(not is_subtype_of(CallableTypeOf[identity], Callable[[str], int]))
```

The reverse is not true — if someone expects a generic function that can be called with any
specialization, we cannot hand them a function that only works with one specialization.

```py
static_assert(not is_subtype_of(Callable[[int], int], TypeOf[identity]))
static_assert(not is_subtype_of(Callable[[str], str], TypeOf[identity]))
static_assert(not is_subtype_of(Callable[[str], int], TypeOf[identity]))

static_assert(not is_subtype_of(Callable[[int], int], CallableTypeOf[identity]))
static_assert(not is_subtype_of(Callable[[str], str], CallableTypeOf[identity]))
static_assert(not is_subtype_of(Callable[[str], int], CallableTypeOf[identity]))
```

[gradual form]: https://typing.python.org/en/latest/spec/glossary.html#term-gradual-form
[gradual tuple]: https://typing.python.org/en/latest/spec/tuples.html#tuple-type-form
[special case for float and complex]: https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
[typing documentation]: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
