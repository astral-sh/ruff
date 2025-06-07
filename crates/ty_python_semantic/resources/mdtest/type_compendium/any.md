# `Any`

## Introduction

The type `Any` is the dynamic type in Python's gradual type system. It represents an unknown
fully-static type, which means that it represents an *unknown* set of runtime values.

```py
from ty_extensions import static_assert, is_fully_static
from typing import Any
```

`Any` is a dynamic type:

```py
static_assert(not is_fully_static(Any))
```

## Every type is assignable to `Any`, and `Any` is assignable to every type

```py
from ty_extensions import static_assert, is_fully_static, is_assignable_to
from typing_extensions import Never, Any

class C: ...

static_assert(is_assignable_to(C, Any))
static_assert(is_assignable_to(Any, C))

static_assert(is_assignable_to(object, Any))
static_assert(is_assignable_to(Any, object))

static_assert(is_assignable_to(Never, Any))
static_assert(is_assignable_to(Any, Never))

static_assert(is_assignable_to(type, Any))
static_assert(is_assignable_to(Any, type))

static_assert(is_assignable_to(type[Any], Any))
static_assert(is_assignable_to(Any, type[Any]))
```

`Any` is also assignable to itself (like every type):

```py
static_assert(is_assignable_to(Any, Any))
```

## Unions with `Any`: `Any | T`

The union `Any | T` of `Any` with a fully static type `T` describes an unknown set of values that is
*at least as large* as the set of values described by `T`. It represents an unknown fully-static
type with *lower bound* `T`. Again, this can be demonstrated using the assignable-to relation:

```py
from ty_extensions import static_assert, is_assignable_to, is_equivalent_to
from typing_extensions import Any

# A class hierarchy Small <: Medium <: Big

class Big: ...
class Medium(Big): ...
class Small(Medium): ...

static_assert(is_assignable_to(Any | Medium, Big))
static_assert(is_assignable_to(Any | Medium, Medium))

# `Any | Medium` is at least as large as `Medium`, so we can not assign it to `Small`:
static_assert(not is_assignable_to(Any | Medium, Small))
```

The union `Any | object` is equivalent to `object`. This is true for every union with `object`, but
it is worth demonstrating:

```py
static_assert(is_equivalent_to(Any | object, object))
static_assert(is_equivalent_to(object | Any, object))
```

## Intersections with `Any`: `Any & T`

The intersection `Any & T` of `Any` with a fully static type `T` describes an unknown set of values
that is *no larger than* the set of values described by `T`. It represents an unknown fully-static
type with *upper bound* `T`:

```py
from ty_extensions import static_assert, is_assignable_to, Intersection, is_equivalent_to
from typing import Any

class Big: ...
class Medium(Big): ...
class Small(Medium): ...

static_assert(is_assignable_to(Small, Intersection[Any, Medium]))
static_assert(is_assignable_to(Medium, Intersection[Any, Medium]))
```

`Any & Medium` is no larger than `Medium`, so we can not assign `Big` to it. There is no possible
materialization of `Any & Medium` that would make it as big as `Big`:

```py
static_assert(not is_assignable_to(Big, Intersection[Any, Medium]))
```

`Any & Never` represents an "unknown" fully-static type which is no larger than `Never`. There is no
such fully-static type, except for `Never` itself. So `Any & Never` is equivalent to `Never`:

```py
from typing_extensions import Never

static_assert(is_equivalent_to(Intersection[Any, Never], Never))
static_assert(is_equivalent_to(Intersection[Never, Any], Never))
```

## Tuples with `Any`

This section demonstrates the following passage from the [type system concepts] documentation on
gradual types:

> A type such as `tuple[int, Any]` […] does not represent a single set of Python objects; rather, it
> represents a (bounded) range of possible sets of values. […] In the same way that `Any` does not
> represent "the set of all Python objects" but rather "an unknown set of objects",
> `tuple[int, Any]` does not represent "the set of all length-two tuples whose first element is an
> integer". That is a fully static type, spelled `tuple[int, object]`. By contrast,
> `tuple[int, Any]` represents some unknown set of tuple values; it might be the set of all tuples
> of two integers, or the set of all tuples of an integer and a string, or some other set of tuple
> values.
>
> In practice, this difference is seen (for example) in the fact that we can assign an expression of
> type `tuple[int, Any]` to a target typed as `tuple[int, int]`, whereas assigning
> `tuple[int, object]` to `tuple[int, int]` is a static type error.

```py
from ty_extensions import static_assert, is_assignable_to
from typing import Any

static_assert(is_assignable_to(tuple[int, Any], tuple[int, int]))
static_assert(not is_assignable_to(tuple[int, object], tuple[int, int]))
```

[type system concepts]: https://typing.readthedocs.io/en/latest/spec/concepts.html#gradual-types
