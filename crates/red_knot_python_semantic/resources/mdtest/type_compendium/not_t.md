# `Not[T]`

The type `Not[T]` is the complement of the type `T`. It describes the set of all values that are
*not* in `T`.

## `Not[T]` is disjoint from `T`

`Not[T]` is disjoint from `T`:

```py
from knot_extensions import Not, static_assert, is_disjoint_from

class T: ...
class S(T): ...

# TODO: These should succeed
# error: [static-assert-error]
static_assert(is_disjoint_from(Not[T], T))
# error: [static-assert-error]
static_assert(is_disjoint_from(Not[T], S))
```

## The union of `T` and `Not[T]` is equivalent to `object`

```py
from knot_extensions import Not, static_assert, is_equivalent_to

class T: ...

static_assert(is_equivalent_to(T | Not[T], object))
```
