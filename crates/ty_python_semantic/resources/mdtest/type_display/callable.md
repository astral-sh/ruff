# Display of callable types

## Parenthesizing callables

### Simple

We parenthesize callable types when they appear inside more complex types, to disambiguate:

```py
from typing import Callable

def f(x: Callable[[], str] | Callable[[int], str]):
    reveal_type(x)  # revealed: (() -> str) | ((int, /) -> str)
```

### Overloaded

We don't parenthesize display of an overloaded callable, since it is already wrapped in
`Overload[...]`:

```py
from typing import overload, Callable
from ty_extensions import CallableTypeOf

@overload
def f(x: int) -> bool: ...
@overload
def f(x: str) -> str: ...
def f(x: int | str) -> bool | str:
    return bool(x) if isinstance(x, int) else str(x)

def _(flag: bool, c: CallableTypeOf[f]):
    x = c if flag else True
    reveal_type(x)  # revealed: Overload[(x: int) -> bool, (x: str) -> str] | Literal[True]
```

### Top

And we don't parenthesize the top callable, since it is wrapped in `Top[...]`:

```py
from typing import Callable
from ty_extensions import Top

def f(x: Top[Callable[..., str]] | Callable[[int], int]):
    reveal_type(x)  # revealed: Top[(...) -> str] | ((int, /) -> int)
```

## Top ParamSpec

```toml
[environment]
python-version = "3.12"
```

We wrap the signature of a top ParamSpec with `Top[...]`:

```py
from typing import Callable

class C[**P]:
    def __init__(self, f: Callable[P, object]) -> None:
        self.f = f

def _(x: object):
    if callable(x):
        c = C(x)
        reveal_type(c)  # revealed: C[Top[(...)]]
```
