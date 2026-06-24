# `~T`

```toml
[environment]
python-version = "3.14"
```

The type `~T` is the complement of the type `T`. It describes the set of all values that are *not*
in `T`.

## `~T` is disjoint from `T`

`~T` is disjoint from `T`:

```pyi
from ty_extensions import static_assert, is_disjoint_from

class T: ...
class S(T): ...

static_assert(is_disjoint_from(~T, T))
static_assert(is_disjoint_from(~T, S))
```

## The union of `T` and `~T` is equivalent to `object`

Together, `T` and `~T` describe the set of all values. So the union of both types is equivalent to
`object`:

```pyi
from ty_extensions import static_assert, is_equivalent_to

class T: ...

static_assert(is_equivalent_to(T | ~T, object))
```

## `~T` reverses subtyping relationships

If `S <: T`, then `~T <: ~S`:, similar to how negation in logic reverses the order of `<=`:

```pyi
from ty_extensions import static_assert, is_subtype_of

class T: ...
class S(T): ...

static_assert(is_subtype_of(S, T))
static_assert(is_subtype_of(~T, ~S))
```

## `~T` reverses assignability relationships

Assignability relationships are similarly reversed:

```pyi
from ty_extensions import static_assert, is_assignable_to
from typing import Any

class T: ...
class S(T): ...

static_assert(is_assignable_to(S, T))
static_assert(is_assignable_to(~T, ~S))

static_assert(is_assignable_to(Any & S, Any & T))

static_assert(is_assignable_to(~(Any & S), ~(Any & T)))
```

## Subtyping and disjointness

If two types `P` and `Q` are disjoint, then `P` must be a subtype of `~Q`, and vice versa:

```pyi
from ty_extensions import static_assert, is_subtype_of, is_disjoint_from
from typing import final

@final
class P: ...

@final
class Q: ...

static_assert(is_disjoint_from(P, Q))

static_assert(is_subtype_of(P, ~Q))
static_assert(is_subtype_of(Q, ~P))
```

## De-Morgan's laws

Given two unrelated types `P` and `Q`, we can demonstrate De-Morgan's laws in the context of
set-theoretic types:

```pyi
from ty_extensions import static_assert, is_equivalent_to

class P: ...
class Q: ...
```

The negation of a union is the intersection of the negations:

```pyi
static_assert(is_equivalent_to(~(P | Q), ~P & ~Q))
```

Conversely, the negation of an intersection is the union of the negations:

```pyi
static_assert(is_equivalent_to(~(P & Q), ~P | ~Q))
```

## Negation of gradual types

`Any` represents an unknown set of values. So `~Any` also represents an unknown set of values. The
two gradual types are equivalent:

```pyi
from ty_extensions import static_assert, is_equivalent_to
from typing import Any

static_assert(is_equivalent_to(~Any, Any))
```
