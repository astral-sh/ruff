# Narrowing for `not` conditionals

The `not` operator negates a constraint.

## `not is None`

```py
from typing import Literal

def _(x: None | Literal[1]):
    if not x is None:
        reveal_type(x)  # revealed: Literal[1]
    else:
        reveal_type(x)  # revealed: None

    reveal_type(x)  # revealed: None | Literal[1]
```

## `not isinstance`

```py
from typing import Literal

def _(x: Literal[1, "a"]):
    if not isinstance(x, (int)):
        reveal_type(x)  # revealed: Literal["a"]
    else:
        reveal_type(x)  # revealed: Literal[1]
```
