# `object`

The `object` type represents the set of all Python objects.

## `object` is a supertype of all types

It is the top type in Python's type system, i.e., it is a supertype of all other types:

```py
from ty_extensions import static_assert, is_subtype_of

static_assert(is_subtype_of(int, object))
static_assert(is_subtype_of(str, object))
static_assert(is_subtype_of(type, object))
static_assert(is_subtype_of(object, object))
```

## Every type is assignable to `object`

Everything can be assigned to the type `object`. This fact can be used to create heterogeneous
collections of objects (but also erases more specific type information):

```py
from ty_extensions import static_assert, is_assignable_to
from typing_extensions import Any, Never

static_assert(is_assignable_to(int, object))
static_assert(is_assignable_to(str | bytes, object))
static_assert(is_assignable_to(type, object))
static_assert(is_assignable_to(object, object))
static_assert(is_assignable_to(Never, object))
static_assert(is_assignable_to(Any, object))

x: list[object] = [1, "a", ()]
```

## `object` overlaps with all types

There is no type that is disjoint from `object` except for `Never`:

```py
from ty_extensions import static_assert, is_disjoint_from
from typing_extensions import Any, Never

static_assert(not is_disjoint_from(int, object))
static_assert(not is_disjoint_from(str, object))
static_assert(not is_disjoint_from(type, object))
static_assert(not is_disjoint_from(object, object))
static_assert(not is_disjoint_from(Any, object))
static_assert(is_disjoint_from(Never, object))
```

## Unions with `object`

Unions with `object` are equivalent to `object`:

```py
from ty_extensions import static_assert, is_equivalent_to

static_assert(is_equivalent_to(int | object | None, object))
```

## Intersections with `object`

Intersecting with `object` is equivalent to the original type:

```py
from ty_extensions import static_assert, is_equivalent_to, Intersection

class P: ...
class Q: ...

static_assert(is_equivalent_to(Intersection[P, object, Q], Intersection[P, Q]))
```

## `object` is the complement of `Never`

See corresponding section in the fact sheet for [`Never`](never.md).
