# Fully-static types

A type is fully static iff it does not contain any gradual forms.

## Fully-static

```py
from typing_extensions import Literal, LiteralString, Never, Callable
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
from typing_extensions import Any, Literal, LiteralString, Callable
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

## Callable

```py
from typing_extensions import Callable, Any
from knot_extensions import Unknown, is_fully_static, static_assert

static_assert(is_fully_static(Callable[[], int]))
static_assert(is_fully_static(Callable[[int, str], int]))

static_assert(not is_fully_static(Callable[..., int]))
static_assert(not is_fully_static(Callable[[], Any]))
static_assert(not is_fully_static(Callable[[int, Unknown], int]))
```

The invalid forms of `Callable` annotation are never fully static because we represent them with the
`(...) -> Unknown` signature.

```py
static_assert(not is_fully_static(Callable))
# error: [invalid-type-form]
static_assert(not is_fully_static(Callable[int, int]))
```

Using function literals, we can check more variations of callable types as it allows us to define
parameters without annotations and no return type.

```py
from knot_extensions import CallableTypeOf, is_fully_static, static_assert

def f00() -> None: ...
def f01(a: int, b: str) -> None: ...
def f11(): ...
def f12(a, b): ...
def f13(a, b: int): ...
def f14(a, b: int) -> None: ...
def f15(a, b) -> None: ...

static_assert(is_fully_static(CallableTypeOf[f00]))
static_assert(is_fully_static(CallableTypeOf[f01]))

static_assert(not is_fully_static(CallableTypeOf[f11]))
static_assert(not is_fully_static(CallableTypeOf[f12]))
static_assert(not is_fully_static(CallableTypeOf[f13]))
static_assert(not is_fully_static(CallableTypeOf[f14]))
static_assert(not is_fully_static(CallableTypeOf[f15]))
```
