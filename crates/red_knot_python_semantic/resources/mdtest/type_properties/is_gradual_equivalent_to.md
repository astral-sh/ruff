# Gradual equivalence relation

Two gradual types `A` and `B` are equivalent if all [materializations] of `A` are also
materializations of `B`, and all materializations of `B` are also materializations of `A`.

## Basic

```py
from typing import Any
from typing_extensions import Literal, LiteralString, Never
from knot_extensions import AlwaysFalsy, AlwaysTruthy, TypeOf, Unknown, is_gradual_equivalent_to, static_assert

static_assert(is_gradual_equivalent_to(Any, Any))
static_assert(is_gradual_equivalent_to(Unknown, Unknown))
static_assert(is_gradual_equivalent_to(Any, Unknown))

static_assert(is_gradual_equivalent_to(Never, Never))
static_assert(is_gradual_equivalent_to(AlwaysTruthy, AlwaysTruthy))
static_assert(is_gradual_equivalent_to(AlwaysFalsy, AlwaysFalsy))
static_assert(is_gradual_equivalent_to(LiteralString, LiteralString))

static_assert(is_gradual_equivalent_to(Literal[True], Literal[True]))
static_assert(is_gradual_equivalent_to(Literal[False], Literal[False]))
static_assert(is_gradual_equivalent_to(TypeOf[0:1:2], TypeOf[0:1:2]))

static_assert(is_gradual_equivalent_to(TypeOf[str], TypeOf[str]))
static_assert(is_gradual_equivalent_to(type, type[object]))

static_assert(not is_gradual_equivalent_to(type, type[Any]))
static_assert(not is_gradual_equivalent_to(type[object], type[Any]))
```

## Unions and intersections

```py
from typing import Any
from knot_extensions import Intersection, Not, Unknown, is_gradual_equivalent_to, static_assert

static_assert(is_gradual_equivalent_to(str | int, str | int))
static_assert(is_gradual_equivalent_to(str | int | Any, str | int | Unknown))
static_assert(is_gradual_equivalent_to(str | int, int | str))
static_assert(
    is_gradual_equivalent_to(Intersection[str, int, Not[bytes], Not[None]], Intersection[int, str, Not[None], Not[bytes]])
)
# TODO: `~type[Any]` shoudld be gradually equivalent to `~type[Unknown]`
# error: [static-assert-error]
static_assert(is_gradual_equivalent_to(Intersection[str | int, Not[type[Any]]], Intersection[int | str, Not[type[Unknown]]]))

static_assert(not is_gradual_equivalent_to(str | int, int | str | bytes))
static_assert(not is_gradual_equivalent_to(str | int | bytes, int | str | dict))
```

## Tuples

```py
from knot_extensions import Unknown, is_gradual_equivalent_to, static_assert

static_assert(is_gradual_equivalent_to(tuple[str, Any], tuple[str, Unknown]))

static_assert(not is_gradual_equivalent_to(tuple[str, int], tuple[str, int, bytes]))
static_assert(not is_gradual_equivalent_to(tuple[str, int], tuple[int, str]))
```

[materializations]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-materialize
