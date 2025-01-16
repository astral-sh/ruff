# Fully-static types

A type is fully static iff it does not contain any gradual forms.

## Fully-static

```py
from typing_extensions import Literal, LiteralString, Never
from knot_extensions import Intersection, Not, TypeOf, is_fully_static, static_assert

static_assert(is_fully_static(Never))
static_assert(is_fully_static(None))

static_assert(is_fully_static(Literal[1]))
static_assert(is_fully_static(Literal[True]))
static_assert(is_fully_static(Literal["abc"]))
static_assert(is_fully_static(Literal[b"abc"]))

static_assert(is_fully_static(LiteralString))

static_assert(is_fully_static(str))
static_assert(is_fully_static(object))
static_assert(is_fully_static(type))

static_assert(is_fully_static(TypeOf[str]))
static_assert(is_fully_static(TypeOf[Literal]))

static_assert(is_fully_static(str | None))
static_assert(is_fully_static(Intersection[str, Not[LiteralString]]))

static_assert(is_fully_static(tuple[()]))
static_assert(is_fully_static(tuple[int, object]))

static_assert(is_fully_static(type[str]))
static_assert(is_fully_static(type[object]))
```

## Non-fully-static

```py
from typing_extensions import Any, Literal, LiteralString
from knot_extensions import Intersection, Not, TypeOf, Unknown, is_fully_static, static_assert

static_assert(not is_fully_static(Any))
static_assert(not is_fully_static(Unknown))

static_assert(not is_fully_static(Any | str))
static_assert(not is_fully_static(str | Unknown))
static_assert(not is_fully_static(Intersection[Any, Not[LiteralString]]))

static_assert(not is_fully_static(tuple[Any, ...]))
static_assert(not is_fully_static(tuple[int, Any]))
static_assert(not is_fully_static(type[Any]))
```
