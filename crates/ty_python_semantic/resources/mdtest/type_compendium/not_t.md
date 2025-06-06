# `Not[T]`

The type `Not[T]` is the complement of the type `T`. It describes the set of all values that are
*not* in `T`.

## `Not[T]` is disjoint from `T`

`Not[T]` is disjoint from `T`:

```py
from ty_extensions import Not, static_assert, is_disjoint_from

class T: ...
class S(T): ...

static_assert(is_disjoint_from(Not[T], T))
static_assert(is_disjoint_from(Not[T], S))
```

## The union of `T` and `Not[T]` is equivalent to `object`

Together, `T` and `Not[T]` describe the set of all values. So the union of both types is equivalent
to `object`:

```py
from ty_extensions import Not, static_assert, is_equivalent_to

class T: ...

static_assert(is_equivalent_to(T | Not[T], object))
```

## `Not[T]` reverses subtyping relationships

If `S <: T`, then `Not[T] <: Not[S]`:, similar to how negation in logic reverses the order of `<=`:

```py
from ty_extensions import Not, static_assert, is_subtype_of

class T: ...
class S(T): ...

static_assert(is_subtype_of(S, T))
static_assert(is_subtype_of(Not[T], Not[S]))
```

## `Not[T]` reverses assignability relationships

Assignability relationships are similarly reversed:

```py
from ty_extensions import Not, Intersection, static_assert, is_assignable_to
from typing import Any

class T: ...
class S(T): ...

static_assert(is_assignable_to(S, T))
static_assert(is_assignable_to(Not[T], Not[S]))

static_assert(is_assignable_to(Intersection[Any, S], Intersection[Any, T]))

static_assert(is_assignable_to(Not[Intersection[Any, S]], Not[Intersection[Any, T]]))
```

## Subtyping and disjointness

If two types `P` and `Q` are disjoint, then `P` must be a subtype of `Not[Q]`, and vice versa:

```py
from ty_extensions import Not, static_assert, is_subtype_of, is_disjoint_from
from typing import final

@final
class P: ...

@final
class Q: ...

static_assert(is_disjoint_from(P, Q))

static_assert(is_subtype_of(P, Not[Q]))
static_assert(is_subtype_of(Q, Not[P]))
```

## De-Morgan's laws

Given two unrelated types `P` and `Q`, we can demonstrate De-Morgan's laws in the context of
set-theoretic types:

```py
from ty_extensions import Not, static_assert, is_equivalent_to, Intersection

class P: ...
class Q: ...
```

The negation of a union is the intersection of the negations:

```py
static_assert(is_equivalent_to(Not[P | Q], Intersection[Not[P], Not[Q]]))
```

Conversely, the negation of an intersection is the union of the negations:

```py
static_assert(is_equivalent_to(Not[Intersection[P, Q]], Not[P] | Not[Q]))
```

## Negation of gradual types

`Any` represents an unknown set of values. So `Not[Any]` also represents an unknown set of values.
The two gradual types are equivalent:

```py
from ty_extensions import static_assert, is_gradual_equivalent_to, Not
from typing import Any

static_assert(is_gradual_equivalent_to(Not[Any], Any))
```
