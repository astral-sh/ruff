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
static_assert(is_gradual_equivalent_to(Intersection[str | int, Not[type[Any]]], Intersection[int | str, Not[type[Unknown]]]))

static_assert(not is_gradual_equivalent_to(str | int, int | str | bytes))
static_assert(not is_gradual_equivalent_to(str | int | bytes, int | str | dict))
```

## Tuples

```py
from knot_extensions import Unknown, is_gradual_equivalent_to, static_assert
from typing import Any

static_assert(is_gradual_equivalent_to(tuple[str, Any], tuple[str, Unknown]))

static_assert(not is_gradual_equivalent_to(tuple[str, int], tuple[str, int, bytes]))
static_assert(not is_gradual_equivalent_to(tuple[str, int], tuple[int, str]))
```

## Callable

```py
from knot_extensions import Unknown, CallableTypeFromFunction, is_gradual_equivalent_to, static_assert
from typing import Any, Callable

static_assert(is_gradual_equivalent_to(Callable[..., int], Callable[..., int]))
static_assert(is_gradual_equivalent_to(Callable[..., Any], Callable[..., Unknown]))
static_assert(is_gradual_equivalent_to(Callable[[int, Any], None], Callable[[int, Unknown], None]))

static_assert(not is_gradual_equivalent_to(Callable[[int, Any], None], Callable[[Any, int], None]))
static_assert(not is_gradual_equivalent_to(Callable[[int, str], None], Callable[[int, str, bytes], None]))
static_assert(not is_gradual_equivalent_to(Callable[..., None], Callable[[], None]))
```

A function with no explicit return type should be gradual equivalent to a callable with a return
type of `Any`.

```py
def f1():
    return

static_assert(is_gradual_equivalent_to(CallableTypeFromFunction[f1], Callable[[], Any]))
```

And, similarly for parameters with no annotations.

```py
def f2(a, b) -> None:
    return

static_assert(is_gradual_equivalent_to(CallableTypeFromFunction[f2], Callable[[Any, Any], None]))
```

Additionally, as per the spec, a function definition that includes both `*args` and `**kwargs`
parameter that are annotated as `Any` or kept unannotated should be gradual equivalent to a callable
with `...` as the parameter type.

```py
def variadic_without_annotation(*args, **kwargs):
    return

def variadic_with_annotation(*args: Any, **kwargs: Any) -> Any:
    return

static_assert(is_gradual_equivalent_to(CallableTypeFromFunction[variadic_without_annotation], Callable[..., Any]))
static_assert(is_gradual_equivalent_to(CallableTypeFromFunction[variadic_with_annotation], Callable[..., Any]))
```

But, a function with either `*args` or `**kwargs` is not gradual equivalent to a callable with `...`
as the parameter type.

```py
def variadic_args(*args):
    return

def variadic_kwargs(**kwargs):
    return

static_assert(not is_gradual_equivalent_to(CallableTypeFromFunction[variadic_args], Callable[..., Any]))
static_assert(not is_gradual_equivalent_to(CallableTypeFromFunction[variadic_kwargs], Callable[..., Any]))
```

[materializations]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-materialize
