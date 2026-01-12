# Narrowing for `callable()`

## Basic narrowing

The `callable()` builtin returns `TypeIs[Callable[..., object]]`, which narrows the type to the
intersection with `Top[Callable[..., object]]`. The `Top[...]` wrapper indicates this is a fully
static type representing the top materialization of a gradual callable.

Since all callable types are subtypes of `Top[Callable[..., object]]`, intersections with `Top[...]`
simplify to just the original callable type.

```py
from typing import Any, Callable

def f(x: Callable[..., Any] | None):
    if callable(x):
        # The intersection simplifies because `(...) -> Any` is a subtype of
        # `Top[(...) -> object]` - all callables are subtypes of the top materialization.
        reveal_type(x)  # revealed: (...) -> Any
    else:
        # Since `(...) -> Any` is a subtype of `Top[(...) -> object]`, the intersection
        # with the negation is empty (Never), leaving just None.
        reveal_type(x)  # revealed: None
```

## Narrowing with other callable types

```py
from typing import Any, Callable

def g(x: Callable[[int], str] | None):
    if callable(x):
        # All callables are subtypes of `Top[(...) -> object]`, so the intersection simplifies.
        reveal_type(x)  # revealed: (int, /) -> str
    else:
        reveal_type(x)  # revealed: None

def h(x: Callable[..., int] | None):
    if callable(x):
        reveal_type(x)  # revealed: (...) -> int
    else:
        reveal_type(x)  # revealed: None
```

## Narrowing from object

```py
def f(x: object):
    if callable(x):
        reveal_type(x)  # revealed: Top[(...) -> object]
    else:
        reveal_type(x)  # revealed: ~Top[(...) -> object]
```

## Calling narrowed callables

The narrowed type `Top[Callable[..., object]]` represents the set of all possible callable types
(including, e.g., functions that take no arguments and functions that require arguments). While such
objects *are* callable (they pass `callable()`), no specific set of arguments can be guaranteed to
be valid.

```py
import typing as t

def call_with_args(y: object, a: int, b: str) -> object:
    if isinstance(y, t.Callable):
        # error: [call-top-callable]
        return y(a, b)
    return None
```

## Assignability of narrowed callables

A narrowed callable `Top[Callable[..., object]]` should be assignable to `Callable[..., Any]`. This
is important for decorators and other patterns where we need to pass the narrowed callable to
functions expecting gradual callables.

```py
from typing import Any, Callable, TypeVar
from ty_extensions import static_assert, Top, is_assignable_to

static_assert(is_assignable_to(Top[Callable[..., bool]], Callable[..., int]))

F = TypeVar("F", bound=Callable[..., Any])

def wrap(f: F) -> F:
    return f

def f(x: object):
    if callable(x):
        # x has type `Top[(...) -> object]`, which should be assignable to `Callable[..., Any]`
        wrap(x)
```
