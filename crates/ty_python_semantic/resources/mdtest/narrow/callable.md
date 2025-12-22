# Narrowing for `callable()`

## Basic narrowing

The `callable()` builtin returns `TypeIs[Callable[..., object]]`, which narrows the type to the
intersection with `Callable[..., object]`.

```py
from typing import Any, Callable

def f(x: Callable[..., Any] | None):
    if callable(x):
        # The intersection of `Callable[..., Any]` with `Callable[..., object]` preserves
        # the gradual parameters (`...`). Previously this was incorrectly narrowed to
        # `((...) -> Any) & (() -> object)` because the top materialization of gradual
        # parameters was incorrectly `[]` instead of `...`.
        reveal_type(x)  # revealed: ((...) -> Any) & ((...) -> object)
    else:
        reveal_type(x)  # revealed: (((...) -> Any) & ~((...) -> object)) | None
```

## Narrowing with other callable types

```py
from typing import Any, Callable

def g(x: Callable[[int], str] | None):
    if callable(x):
        reveal_type(x)  # revealed: ((int, /) -> str) & ((...) -> object)
    else:
        reveal_type(x)  # revealed: (((int, /) -> str) & ~((...) -> object)) | None

def h(x: Callable[..., int] | None):
    if callable(x):
        reveal_type(x)  # revealed: ((...) -> int) & ((...) -> object)
    else:
        reveal_type(x)  # revealed: (((...) -> int) & ~((...) -> object)) | None
```

## Narrowing from object

```py
from typing import Callable

def f(x: object):
    if callable(x):
        reveal_type(x)  # revealed: (...) -> object
    else:
        reveal_type(x)  # revealed: ~((...) -> object)
```
