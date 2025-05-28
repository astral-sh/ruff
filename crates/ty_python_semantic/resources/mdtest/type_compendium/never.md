# `Never`

`Never` represents the empty set of values.

## `Never` is a subtype of every type

The `Never` type is the bottom type of Python's type system. It is a subtype of every type, but no
type is a subtype of `Never`, except for `Never` itself.

```py
from ty_extensions import static_assert, is_subtype_of
from typing_extensions import Never

class C: ...

static_assert(is_subtype_of(Never, int))
static_assert(is_subtype_of(Never, object))
static_assert(is_subtype_of(Never, C))
static_assert(is_subtype_of(Never, Never))

static_assert(not is_subtype_of(int, Never))
```

## `Never` is assignable to every type

`Never` is assignable to every type. This fact is useful when calling error-handling functions in a
context that requires a value of a specific type. For example, changing the `Never` return type to
`None` below would cause a type error:

```py
from ty_extensions import static_assert, is_assignable_to
from typing_extensions import Never, Any

static_assert(is_assignable_to(Never, int))
static_assert(is_assignable_to(Never, object))
static_assert(is_assignable_to(Never, Any))
static_assert(is_assignable_to(Never, Never))

def raise_error() -> Never:
    raise Exception("...")

def f(divisor: int) -> None:
    x: float = (1 / divisor) if divisor != 0 else raise_error()
```

## `Never` in annotations

`Never` can be used in functions to indicate that the function never returns. For example, if a
function always raises an exception, if it calls `sys.exit()`, if it enters an infinite loop, or if
it calls itself recursively. All of these functions "Never" return control back to the caller:

```py
from typing_extensions import Never

def raises_unconditionally() -> Never:
    raise Exception("This function always raises an exception")

def exits_unconditionally() -> Never:
    import sys

    return sys.exit(1)

def loops_forever() -> Never:
    while True:
        pass

def recursive_never() -> Never:
    return recursive_never()
```

Similarly, if `Never` is used in parameter positions, it indicates that the function can "Never" be
called, because it can never be passed a value of type `Never` (there are none):

```py
def can_not_be_called(n: Never) -> int:
    return 0
```

## `Never` is disjoint from every other type

Two types `A` and `B` are disjoint if their intersection is empty. Since `Never` has no inhabitants,
it is disjoint from every other type:

```py
from ty_extensions import static_assert, is_disjoint_from
from typing_extensions import Never

class C: ...

static_assert(is_disjoint_from(Never, int))
static_assert(is_disjoint_from(Never, object))
static_assert(is_disjoint_from(Never, C))
static_assert(is_disjoint_from(Never, Never))
```

## Unions with `Never`

`Never` can always be removed from unions:

```py
from ty_extensions import static_assert, is_equivalent_to
from typing_extensions import Never

class P: ...
class Q: ...

static_assert(is_equivalent_to(P | Never | Q | None, P | Q | None))
```

## Intersections with `Never`

Intersecting with `Never` results in `Never`:

```py
from ty_extensions import static_assert, is_equivalent_to, Intersection
from typing_extensions import Never

class P: ...
class Q: ...

static_assert(is_equivalent_to(Intersection[P, Never, Q], Never))
```

## `Never` is the complement of `object`

`object` describes the set of all possible values, while `Never` describes the empty set. The two
types are complements of each other:

```py
from ty_extensions import static_assert, is_equivalent_to, Not
from typing_extensions import Never

static_assert(is_equivalent_to(Not[object], Never))
static_assert(is_equivalent_to(Not[Never], object))
```

This duality is also reflected in other facts:

- `Never` is a subtype of every type, while `object` is a supertype of every type.
- `Never` is assignable to every type, while `object` is assignable from every type.
- `Never` is disjoint from every type, while `object` overlaps with every type.
- Building a union with `Never` is a no-op, intersecting with `object` is a no-op.
- Interecting with `Never` results in `Never`, building a union with `object` results in `object`.

## Lists of `Never`

`list[Never]` is a reasonable type that is *not* equivalent to `Never`. The empty list inhabits this
type:

```py
from typing_extensions import Never

x: list[Never] = []
```

## Tuples involving `Never`

A type like `tuple[int, Never]` has no inhabitants, and so it is equivalent to `Never`:

```py
from ty_extensions import static_assert, is_equivalent_to
from typing_extensions import Never

static_assert(is_equivalent_to(tuple[int, Never], Never))
```

Note that this is not the case for the homogenous tuple type `tuple[Never, ...]` though, because
that type is inhabited by the empty tuple:

```py
static_assert(not is_equivalent_to(tuple[Never, ...], Never))

t: tuple[Never, ...] = ()
```

## `NoReturn` is the same as `Never`

The `NoReturn` type is a different name for `Never`:

```py
from ty_extensions import static_assert, is_equivalent_to
from typing_extensions import NoReturn, Never

static_assert(is_equivalent_to(NoReturn, Never))
```
