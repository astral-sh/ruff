# `typing.Concatenate`

`Concatenate` is used with `Callable` and `ParamSpec` to describe higher-order functions that add,
remove, or transform parameters of other callables.

## Basic `Callable[Concatenate[..., ...], ...]` types

### With ellipsis (gradual form)

```py
from typing_extensions import Callable, Concatenate

def _(c: Callable[Concatenate[int, ...], str]):
    reveal_type(c)  # revealed: (int, /, *args: Any, **kwargs: Any) -> str

def _(c: Callable[Concatenate[int, str, ...], bool]):
    reveal_type(c)  # revealed: (int, str, /, *args: Any, **kwargs: Any) -> bool
```

### With `ParamSpec`

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import Callable, Concatenate, ParamSpec

P = ParamSpec("P")

def _(c: Callable[Concatenate[int, P], str]):
    reveal_type(c)  # revealed: (int, /, *args: P@_.args, **kwargs: P@_.kwargs) -> str
```

## Decorator that strips a prefix parameter

A common use case is decorators that strip the first parameter from a callable.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable, reveal_type
from typing_extensions import Concatenate, ParamSpec

P = ParamSpec("P")

def with_request[**P, R](f: Callable[Concatenate[int, P], R]) -> Callable[P, R]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        return f(0, *args, **kwargs)
    return wrapper

@with_request
def handler(request: int, name: str) -> bool:
    return True

# The decorator strips the first `int` parameter
reveal_type(handler)  # revealed: (name: str) -> bool

# Calling without the stripped parameter should work
handler("test")
```

## Multiple prefix parameters

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable, reveal_type
from typing_extensions import Concatenate, ParamSpec

P = ParamSpec("P")

def add_two_params[**P, R](
    f: Callable[Concatenate[int, str, P], R],
) -> Callable[P, R]:
    def wrapper(*args: P.args, **kwargs: P.kwargs) -> R:
        return f(0, "", *args, **kwargs)
    return wrapper

@add_two_params
def process(a: int, b: str, flag: bool) -> None:
    pass

reveal_type(process)  # revealed: (flag: bool) -> None

process(True)
```

## Assignability of `Concatenate` gradual forms

When both sides of an assignment use `Concatenate[T, ...]`, the prefix parameters must be
compatible. The gradual tail (`...`) still allows assignability for the remaining parameters.

```py
from typing_extensions import Callable, Concatenate

def _(
    x: Callable[Concatenate[int, ...], None],
    y: Callable[Concatenate[str, ...], None],
    same: Callable[Concatenate[int, ...], None],
    gradual: Callable[..., None],
    multi_self: Callable[Concatenate[int, str, ...], None],
    multi_other: Callable[Concatenate[str, int, ...], None],
):
    # Same prefix types: assignable
    x = same

    # Different prefix types: not assignable
    x = y  # error: [invalid-assignment]

    # Swapped multi-prefix types: not assignable
    multi_self = multi_other  # error: [invalid-assignment]

    # Pure gradual is assignable to/from Concatenate gradual
    x = gradual
    gradual = x
```
