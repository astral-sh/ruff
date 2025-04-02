# Narrowing with assert statements

## `assert` with None

```py
def _(x: str | None, y: str | None):
    assert x is not None
    reveal_type(x)  # revealed: str
    assert y is None
    reveal_type(y)  # revealed: None
```

## `assert` on bool

```py
def _(x: bool, y: bool):
    assert x
    reveal_type(x)  # revealed: Literal[True]
    assert not y
    reveal_type(y)  # revealed: Literal[False]
```

## `assert` on `Literal`

```py
from typing import Literal

def _(x: Literal[1, 2, 3]):
    assert x is 2
    reveal_type(x)  # revealed: Literal[2]
```
