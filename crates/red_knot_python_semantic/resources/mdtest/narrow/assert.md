# Narrowing with assert statements

## `assert` a value `is None` or `is not None`

```py
def _(x: str | None, y: str | None):
    assert x is not None
    reveal_type(x)  # revealed: str
    assert y is None
    reveal_type(y)  # revealed: None
```

## `assert` a value is truthy or falsy

```py
def _(x: bool, y: bool):
    assert x
    reveal_type(x)  # revealed: Literal[True]
    assert not y
    reveal_type(y)  # revealed: Literal[False]
```

## `assert` with `is` and `==` for literals

```py
from typing import Literal

def _(x: Literal[1, 2, 3], y: Literal[1, 2, 3]):
    assert x is 2
    reveal_type(x)  # revealed: Literal[2]
    assert y == 2
    reveal_type(y)  # revealed: Literal[1, 2, 3]
```

## `assert` with `isinstance`

```py
def _(x: int | str):
    assert isinstance(x, int)
    reveal_type(x)  # revealed: int
```

## `assert` a value `in` a tuple

```py
from typing import Literal

def _(x: Literal[1, 2, 3], y: Literal[1, 2, 3]):
    assert x in (1, 2)
    reveal_type(x)  # revealed: Literal[1, 2]
    assert y not in (1, 2)
    reveal_type(y)  # revealed: Literal[3]
```
