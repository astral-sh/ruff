# Narrowing for `callable()`

## Basic narrowing

The `callable()` builtin returns `TypeIs[Callable[..., object]]`, which narrows the type to the
intersection with `Top[Callable[..., object]]`. The `Top[...]` wrapper indicates this is a fully
static type representing the top materialization of a gradual callable.

```py
from typing import Any, Callable

def f(x: Callable[..., Any] | None):
    if callable(x):
        # The intersection of `Callable[..., Any]` with `Top[Callable[..., object]]` preserves
        # the gradual parameters (`...`). Previously this was incorrectly narrowed to
        # `((...) -> Any) & (() -> object)` because the top materialization of gradual
        # parameters was incorrectly `[]` instead of `...`.
        reveal_type(x)  # revealed: ((...) -> Any) & (Top[(...) -> object])
    else:
        reveal_type(x)  # revealed: (((...) -> Any) & ~(Top[(...) -> object])) | None
```

## Narrowing with other callable types

```py
from typing import Any, Callable

def g(x: Callable[[int], str] | None):
    if callable(x):
        reveal_type(x)  # revealed: ((int, /) -> str) & (Top[(...) -> object])
    else:
        reveal_type(x)  # revealed: (((int, /) -> str) & ~(Top[(...) -> object])) | None

def h(x: Callable[..., int] | None):
    if callable(x):
        reveal_type(x)  # revealed: ((...) -> int) & (Top[(...) -> object])
    else:
        reveal_type(x)  # revealed: (((...) -> int) & ~(Top[(...) -> object])) | None
```

## Narrowing from object

```py
from typing import Callable

def f(x: object):
    if callable(x):
        reveal_type(x)  # revealed: Top[(...) -> object]
    else:
        reveal_type(x)  # revealed: ~(Top[(...) -> object])
```

## Calling narrowed callables

The narrowed type preserves gradual parameters, so calling with any arguments is valid:

```py
import typing as t

def call_with_args(y: object, a: int, b: str) -> object:
    if isinstance(y, t.Callable):
        # Previously, `y` was incorrectly narrowed to `() -> object`, which caused
        # false-positive "too many positional arguments" errors here.
        return y(a, b)
    return None
```
