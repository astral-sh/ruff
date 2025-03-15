# Equivalence relation

`is_equivalent_to` implements [the equivalence relation] for fully static types.

Two types `A` and `B` are equivalent iff `A` is a subtype of `B` and `B` is a subtype of `A`.

## Basic

```py
from typing import Any
from typing_extensions import Literal
from knot_extensions import Unknown, is_equivalent_to, static_assert

static_assert(is_equivalent_to(Literal[1, 2], Literal[1, 2]))
static_assert(is_equivalent_to(type[object], type))

static_assert(not is_equivalent_to(Any, Any))
static_assert(not is_equivalent_to(Unknown, Unknown))
static_assert(not is_equivalent_to(Any, None))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 0]))
static_assert(not is_equivalent_to(Literal[1, 2], Literal[1, 2, 3]))
```

## Equivalence is commutative

```py
from typing_extensions import Literal
from knot_extensions import is_equivalent_to, static_assert

static_assert(is_equivalent_to(type, type[object]))
static_assert(not is_equivalent_to(Literal[1, 0], Literal[1, 2]))
static_assert(not is_equivalent_to(Literal[1, 2, 3], Literal[1, 2]))
```

## Differently ordered intersections and unions are equivalent

```py
from knot_extensions import is_equivalent_to, static_assert, Intersection, Not

class P: ...
class Q: ...
class R: ...
class S: ...

static_assert(is_equivalent_to(P | Q | R, P | R | Q))  # 1
static_assert(is_equivalent_to(P | Q | R, Q | P | R))  # 2
static_assert(is_equivalent_to(P | Q | R, Q | R | P))  # 3
static_assert(is_equivalent_to(P | Q | R, R | P | Q))  # 4
static_assert(is_equivalent_to(P | Q | R, R | Q | P))  # 5
static_assert(is_equivalent_to(P | R | Q, Q | P | R))  # 6
static_assert(is_equivalent_to(P | R | Q, Q | R | P))  # 7
static_assert(is_equivalent_to(P | R | Q, R | P | Q))  # 8
static_assert(is_equivalent_to(P | R | Q, R | Q | P))  # 9
static_assert(is_equivalent_to(Q | P | R, Q | R | P))  # 10
static_assert(is_equivalent_to(Q | P | R, R | P | Q))  # 11
static_assert(is_equivalent_to(Q | P | R, R | Q | P))  # 12
static_assert(is_equivalent_to(Q | R | P, R | P | Q))  # 13
static_assert(is_equivalent_to(Q | R | P, R | Q | P))  # 14
static_assert(is_equivalent_to(R | P | Q, R | Q | P))  # 15

static_assert(is_equivalent_to(str | None, None | str))

static_assert(is_equivalent_to(Intersection[P, Q], Intersection[Q, P]))
static_assert(is_equivalent_to(Intersection[Q, Not[P]], Intersection[Not[P], Q]))
static_assert(is_equivalent_to(Intersection[Q, R, Not[P]], Intersection[Not[P], R, Q]))
static_assert(is_equivalent_to(Intersection[Q | R, Not[P | S]], Intersection[Not[S | P], R | Q]))
```

## Tuples containing equivalent but differently ordered unions/intersections are equivalent

```py
from knot_extensions import is_equivalent_to, TypeOf, static_assert, Intersection, Not
from typing import Literal

class P: ...
class Q: ...
class R: ...
class S: ...

static_assert(is_equivalent_to(tuple[P | Q], tuple[Q | P]))
static_assert(is_equivalent_to(tuple[P | None], tuple[None | P]))
static_assert(
    is_equivalent_to(tuple[Intersection[P, Q] | Intersection[R, Not[S]]], tuple[Intersection[Not[S], R] | Intersection[Q, P]])
)
```

## Unions containing tuples containing tuples containing unions (etc.)

```py
from knot_extensions import is_equivalent_to, static_assert, Intersection

class P: ...
class Q: ...

static_assert(
    is_equivalent_to(
        tuple[tuple[tuple[P | Q]]] | P,
        tuple[tuple[tuple[Q | P]]] | P,
    )
)
static_assert(
    is_equivalent_to(
        tuple[tuple[tuple[tuple[tuple[Intersection[P, Q]]]]]],
        tuple[tuple[tuple[tuple[tuple[Intersection[Q, P]]]]]],
    )
)
```

## Intersections containing tuples containing unions

```py
from knot_extensions import is_equivalent_to, static_assert, Intersection

class P: ...
class Q: ...
class R: ...

static_assert(is_equivalent_to(Intersection[tuple[P | Q], R], Intersection[tuple[Q | P], R]))
```

## Transformation by intersection

```py
from knot_extensions import Intersection, Not, AlwaysTruthy, AlwaysFalsy, static_assert, is_equivalent_to
from typing_extensions import Literal, LiteralString

# `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
static_assert(is_equivalent_to(Intersection[LiteralString, AlwaysTruthy], Intersection[LiteralString, Not[Literal[""]]]))
# `LiteralString & ~AlwaysTruthy` -> `Literal[""]`
static_assert(is_equivalent_to(Intersection[LiteralString, Not[AlwaysTruthy]], Literal[""]))
# `LiteralString & AlwaysFalsy` -> `Literal[""]`
static_assert(is_equivalent_to(Intersection[LiteralString, AlwaysFalsy], Literal[""]))
# `LiteralString & ~AlwaysFalsy`  -> `LiteralString & ~Literal[""]`
static_assert(is_equivalent_to(Intersection[LiteralString, Not[AlwaysFalsy]], Intersection[LiteralString, Not[Literal[""]]]))
# `bool & ~AlwaysFalsy`, `bool & ~Literal[False]` -> `bool & Literal[True]`
static_assert(is_equivalent_to(Intersection[bool, Not[AlwaysFalsy]], Literal[True]))
static_assert(is_equivalent_to(Intersection[bool, Not[Literal[False]]], Literal[True]))
# `bool & ~AlwaysTruthy`, `bool & ~Literal[True]` -> `bool & Literal[False]`
static_assert(is_equivalent_to(Intersection[bool, Not[AlwaysTruthy]], Literal[False]))
static_assert(is_equivalent_to(Intersection[bool, Not[Literal[True]]], Literal[False]))
```

[the equivalence relation]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-equivalent
